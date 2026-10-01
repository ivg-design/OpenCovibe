use super::{
    github, github_tasks, governance,
    models::{CreateRequestInput, Participant, Room},
    operations,
    store::RoomStore,
};
use serde_json::{json, Value};
use std::{
    io::Write,
    path::{Path, PathBuf},
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const PROTOCOL_VERSION: &str = "2025-06-18";
const DB_NAME: &str = "rooms.sqlite3";
pub const ROOM_TOOL_NAMES: &[&str] = &[
    "snapshot",
    "create_sidechat",
    "read_sidechat",
    "post_message",
    "read_task",
    "create_task",
    "update_task",
    "convert_draft_task",
    "claim_task",
    "finish_task",
    "block_task",
    "request_agent",
    "request_decision",
    "request_review",
    "propose_completion",
    "respond_request",
    "release_task",
];

fn open_store(data_dir: &Path) -> Result<RoomStore, String> {
    if !data_dir.is_absolute() {
        return Err("room MCP data directory must be absolute".into());
    }
    RoomStore::open(&data_dir.join(DB_NAME))
}

pub async fn serve(
    data_dir: PathBuf,
    room_id: String,
    participant_id: String,
) -> Result<(), String> {
    if !data_dir.is_absolute() {
        return Err("room MCP data directory must be absolute".into());
    }
    // Validate the fixed scope before accepting requests. Per-call identifiers are checked again.
    let store = open_store(&data_dir)?;
    let room = store.get(&room_id)?;
    if !room.participants.iter().any(|p| p.id == participant_id) {
        return Err("participant is not a member of this room".into());
    }
    drop(store);

    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin);
    let mut stdout = tokio::io::stdout();
    loop {
        let Some(line) = read_bounded_line(&mut reader).await? else {
            return Ok(());
        };
        let parsed = match serde_json::from_slice::<Value>(&line) {
            Ok(request) => request,
            Err(error) => {
                write_json_line(&mut stdout, &json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":format!("parse error: {error}")}})).await?;
                continue;
            }
        };
        if parsed.is_array() {
            write_json_line(&mut stdout, &json!({"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"batch requests are not supported"}})).await?;
            continue;
        }
        if parsed.get("id").is_none() {
            // MCP notifications, including notifications/initialized, have no response.
            continue;
        }
        let id = parsed.get("id").cloned().unwrap_or(Value::Null);
        let response = dispatch(&data_dir, &room_id, &participant_id, &parsed).await;
        let packet = match response {
            Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
            Err((code, message)) => {
                json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
            }
        };
        write_json_line(&mut stdout, &packet).await?;
    }
}

async fn read_bounded_line<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
) -> Result<Option<Vec<u8>>, String> {
    let mut line = Vec::new();
    loop {
        let available = reader.fill_buf().await.map_err(|e| e.to_string())?;
        if available.is_empty() {
            return if line.is_empty() {
                Ok(None)
            } else {
                Ok(Some(line))
            };
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let take = newline.map_or(available.len(), |index| index + 1);
        if line.len().saturating_add(take) > MAX_REQUEST_BYTES {
            return Err("MCP request exceeds the 1 MiB limit".into());
        }
        let finished = newline.is_some();
        line.extend_from_slice(&available[..take]);
        reader.consume(take);
        if finished {
            if line.last() == Some(&b'\n') {
                line.pop();
            }
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            return Ok(Some(line));
        }
    }
}

async fn write_json_line<W: tokio::io::AsyncWrite + Unpin>(
    writer: &mut W,
    value: &Value,
) -> Result<(), String> {
    let mut bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    writer.write_all(&bytes).await.map_err(|e| e.to_string())?;
    writer.flush().await.map_err(|e| e.to_string())
}

async fn dispatch(
    data_dir: &Path,
    room_id: &str,
    participant_id: &str,
    request: &Value,
) -> Result<Value, (i64, String)> {
    if request.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err((-32600, "jsonrpc must be 2.0".into()));
    }
    let method = request
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| (-32600, "method is required".into()))?;
    match method {
        "ping" => Ok(json!({})),
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {"tools": {"listChanged": false}},
            "serverInfo": {"name":"opencovibe-room","version":env!("CARGO_PKG_VERSION")}
        })),
        "tools/list" => Ok(json!({"tools": tool_definitions()})),
        "tools/call" => {
            let params = request
                .get("params")
                .and_then(Value::as_object)
                .ok_or_else(|| (-32602, "params must be an object".into()))?;
            let tool_name = params
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| (-32602, "tool name is required".into()))?;
            let arguments = params
                .get("arguments")
                .and_then(Value::as_object)
                .ok_or_else(|| (-32602, "tool arguments must be an object".into()))?;
            if arguments.get("room_id").and_then(Value::as_str) != Some(room_id)
                || arguments.get("participant_id").and_then(Value::as_str) != Some(participant_id)
            {
                return Ok(tool_error(
                    "room_id or participant_id does not match this scoped server",
                ));
            }
            match call_tool(
                data_dir,
                room_id,
                participant_id,
                tool_name,
                &Value::Object(arguments.clone()),
            )
            .await
            {
                Ok(value) => Ok(tool_success(value)),
                Err(error) => Ok(tool_error(&error)),
            }
        }
        _ => Err((-32601, format!("method not found: {method}"))),
    }
}

fn tool_success(value: Value) -> Value {
    json!({"content":[{"type":"text","text":value.to_string()}],"isError":false})
}
fn tool_error(message: &str) -> Value {
    json!({"content":[{"type":"text","text":message}],"isError":true})
}
fn required_string<'a>(args: &'a Value, name: &str) -> Result<&'a str, String> {
    args.get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("{name} is required"))
}
fn task_references(args: &Value) -> Result<Vec<String>, String> {
    let Some(items) = args.get("references") else {
        return Ok(vec![]);
    };
    let items = items
        .as_array()
        .ok_or("references must be an array of strings")?;
    if items.len() > 30 {
        return Err("at most 30 references are allowed".into());
    }
    items
        .iter()
        .map(|item| {
            let value = item.as_str().ok_or("references must be strings")?.trim();
            if value.is_empty() || value.len() > 500 {
                return Err("each reference must contain 1–500 bytes".into());
            }
            Ok(value.to_owned())
        })
        .collect()
}
fn with_references(text: &str, references: &[String]) -> String {
    if references.is_empty() {
        text.to_owned()
    } else {
        format!(
            "{}\n\nReferences:\n{}",
            text,
            references
                .iter()
                .map(|reference| format!("- {reference}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}

async fn call_tool(
    data_dir: &Path,
    room_id: &str,
    participant_id: &str,
    name: &str,
    args: &Value,
) -> Result<Value, String> {
    let store = open_store(data_dir)?;
    let room = store.get(room_id)?;
    let peer = room
        .participants
        .iter()
        .find(|p| p.id == participant_id)
        .ok_or("participant is not in this room")?;
    match name {
        "snapshot" => Ok(snapshot(&room, peer)),
        "create_sidechat" => {
            operations::active_peer(&room, participant_id)?;
            let source_message_id = required_string(args, "source_message_id")?;
            let title = required_string(args, "title")?;
            let ids: Vec<String> = args
                .get("participant_ids")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            let source = room
                .messages
                .iter()
                .find(|m| m.id == source_message_id)
                .ok_or("source message not found in this room")?;
            let source_visible = (source.targets(participant_id)
                || source.participant_id.as_deref() == Some(participant_id))
                && source
                    .sidechat_id
                    .as_ref()
                    .is_none_or(|source_sidechat_id| {
                        room.sidechats.iter().any(|s| {
                            &s.id == source_sidechat_id && s.participant_ids.contains(&peer.id)
                        })
                    });
            if !source_visible {
                return Err("source message is not visible to this participant".into());
            }
            let updated = store.create_sidechat(room_id, source_message_id, title, ids)?;
            Ok(json!({"sidechat":updated.sidechats.last()}))
        }
        "read_sidechat" => {
            let id = required_string(args, "sidechat_id")?;
            let sidechat = room
                .sidechats
                .iter()
                .find(|s| s.id == id)
                .ok_or("sidechat not found")?;
            if !sidechat.participant_ids.contains(&peer.id) {
                return Err("participant is not a member of this sidechat".into());
            }
            let source = room
                .messages
                .iter()
                .find(|m| m.id == sidechat.source_message_id)
                .ok_or("sidechat source message is unavailable")?;
            let messages = room
                .messages
                .iter()
                .filter(|m| m.sidechat_id.as_deref() == Some(id) && m.targets(&peer.id))
                .collect::<Vec<_>>();
            Ok(json!({"sidechat":sidechat,"source":source,"messages":messages}))
        }
        "read_task" => {
            let project = room.project.as_ref().ok_or("room has no GitHub Project")?;
            let task_id = required_string(args, "task_id")?;
            if !room.board.items.iter().any(|item| item.id == task_id) {
                return Err("task is not present on this room's board".into());
            }
            github_tasks::read_task_with_progress(project, task_id).await
        }
        "post_message" => {
            operations::active_peer(&room, participant_id)?;
            let body = required_string(args, "body")?;
            let target_id = args
                .get("target_participant_id")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let sidechat_id = match args.get("sidechat_id") {
                Some(Value::Null) => None,
                Some(v) => Some(
                    v.as_str()
                        .ok_or("sidechat_id must be a string or null")?
                        .to_owned(),
                ),
                None => peer.active_sidechat_id.clone(),
            };
            if let Some(ref id) = sidechat_id {
                let sc = room
                    .sidechats
                    .iter()
                    .find(|s| &s.id == id)
                    .ok_or("sidechat not found")?;
                if !sc.participant_ids.contains(&peer.id) {
                    return Err("participant is not a member of this sidechat".into());
                }
                if target_id
                    .as_ref()
                    .is_some_and(|target| !sc.participant_ids.contains(target))
                {
                    return Err("target participant is not a member of this sidechat".into());
                }
            }
            let updated = store.append_message_in_sidechat(
                room_id,
                &peer.name,
                body.to_owned(),
                Some(participant_id.to_owned()),
                target_id,
                None,
                sidechat_id,
            )?;
            Ok(json!({"message_posted":true,"message":updated.messages.last()}))
        }
        "create_task" => {
            operations::active_peer(&room, participant_id)?;
            let project = room.project.as_ref().ok_or("room has no GitHub Project")?;
            let title = required_string(args, "title")?;
            let body = required_string(args, "body")?;
            let assigned_agent = args.get("assigned_agent").and_then(Value::as_str);
            if assigned_agent
                .is_some_and(|name| !room.participants.iter().any(|peer| peer.name == name))
            {
                return Err("assigned_agent must name a current room participant".into());
            }
            let priority = args.get("priority").and_then(Value::as_str);
            if title.chars().count() > 200 || body.len() > 32_000 {
                return Err(
                    "task title must be at most 200 characters and body at most 32000 bytes".into(),
                );
            }
            github_tasks::validate_creation_input(project, body, assigned_agent, priority).await?;
            let first_attempt = store.begin_task_creation(room_id, title, body)?;
            let existing_id = store.created_task_id(room_id, title)?;
            let already_recorded = existing_id.is_some();
            let task_id = if let Some(id) = existing_id {
                github_tasks::read_task(project, &id).await?;
                id
            } else {
                github_tasks::create_task(project, title, body, first_attempt).await?
            };
            if !already_recorded {
                github_tasks::set_task_metadata(project, &task_id, assigned_agent, priority)
                    .await?;
            }
            store.complete_task_creation(room_id, title, &task_id)?;
            let before = store.get(room_id)?.board;
            match github::read_board_containing(&project.id, &task_id).await {
                Ok(board) => {
                    let updated = store.apply_board_snapshot(room_id, &before, board)?;
                    if !updated.board.items.iter().any(|item| item.id == task_id) {
                        return Err(format!("task {task_id} was created, but a concurrent board update won. Retry create_task with the identical title/body to reconcile; no duplicate will be created."));
                    }
                }
                Err(error) => {
                    return Err(format!(
                        "task {task_id} was created, but refreshing the room board failed: {error}"
                    ))
                }
            }
            Ok(json!({"task_id":task_id,"title":title}))
        }
        "update_task" => {
            operations::active_peer(&room, participant_id)?;
            let project = room.project.as_ref().ok_or("room has no GitHub Project")?;
            let task_id = required_string(args, "task_id")?;
            let claim = room
                .claims
                .iter()
                .find(|claim| {
                    claim.task_id == task_id
                        && claim.participant_id == participant_id
                        && matches!(claim.state.as_str(), "active" | "reserved" | "completing")
                })
                .ok_or("this participant must hold an active claim before updating a task")?;
            let progress = required_string(args, "progress")?;
            let evidence = required_string(args, "evidence")?;
            let references = task_references(args)?;
            github_tasks::record_progress(project, task_id, progress, evidence, &references)
                .await?;
            let visible_evidence = with_references(evidence, &references);
            store.update(room_id, |room| {
                let claim = room
                    .claims
                    .iter_mut()
                    .find(|claim| {
                        claim.task_id == task_id
                            && claim.participant_id == participant_id
                            && matches!(claim.state.as_str(), "active" | "reserved" | "completing")
                    })
                    .ok_or("claim changed while recording progress; GitHub comment was saved")?;
                claim.summary = Some(progress.to_owned());
                claim.evidence = Some(visible_evidence.clone());
                claim.updated_at = crate::models::now_iso();
                Ok(())
            })?;
            let before = store.get(room_id)?.board;
            let board = github::read_board(&project.id).await?;
            store.apply_board_snapshot(room_id, &before, board)?;
            Ok(json!({"task_id":task_id,"claim_state":claim.state,"progress_recorded":true}))
        }
        "convert_draft_task" => {
            operations::active_peer(&room, participant_id)?;
            let project = room.project.as_ref().ok_or("room has no GitHub Project")?;
            let task_id = required_string(args, "task_id")?;
            let body = required_string(args, "body")?;
            if !room.claims.iter().any(|claim| {
                claim.task_id == task_id
                    && claim.participant_id == participant_id
                    && matches!(claim.state.as_str(), "active" | "reserved")
            }) {
                return Err("claim this task before converting its draft Issue".into());
            }
            if !room
                .board
                .items
                .iter()
                .any(|item| item.id == task_id && matches!(item.kind.as_str(), "draft" | "issue"))
            {
                return Err("task is not a visible draft or Issue on this room board".into());
            }
            github_tasks::convert_draft_task(project, task_id, body).await?;
            let before = store.get(room_id)?.board;
            let board = github::read_board_containing(&project.id, task_id).await?;
            store.apply_board_snapshot(room_id, &before, board)?;
            Ok(json!({"task_id":task_id,"converted":true}))
        }
        "claim_task" => {
            operations::active_peer(&room, participant_id)?;
            let task_id = required_string(args, "task_id")?;
            let updated = operations::claim_task(&store, room_id, participant_id, task_id).await?;
            Ok(
                json!({"task_id":task_id,"claim":updated.claims.iter().find(|c| c.task_id == task_id)}),
            )
        }
        "finish_task" => {
            operations::active_peer(&room, participant_id)?;
            let task_id = required_string(args, "task_id")?;
            if room
                .board
                .items
                .iter()
                .any(|item| item.id == task_id && item.kind == "draft")
            {
                return Err("convert and enrich this Project draft before finishing it".into());
            }
            let summary = required_string(args, "summary")?;
            let evidence = required_string(args, "evidence")?;
            let references = task_references(args)?;
            let evidence = with_references(evidence, &references);
            let updated = operations::finish_task(
                &store,
                room_id,
                participant_id,
                task_id,
                summary,
                &evidence,
            )
            .await?;
            Ok(
                json!({"task_id":task_id,"claim":updated.claims.iter().find(|c| c.task_id == task_id)}),
            )
        }
        "block_task" => {
            operations::active_peer(&room, participant_id)?;
            let task_id = required_string(args, "task_id")?;
            if !room.claims.iter().any(|claim| {
                claim.task_id == task_id
                    && claim.participant_id == participant_id
                    && !matches!(claim.state.as_str(), "done" | "released")
            }) {
                return Err(
                    "this participant does not own an unfinished claim for the task".into(),
                );
            }
            let reason = required_string(args, "reason")?;
            let references = task_references(args)?;
            let tracking_error = if room
                .board
                .items
                .iter()
                .any(|item| item.id == task_id && item.kind == "issue")
            {
                let project = room.project.as_ref().ok_or("room has no GitHub Project")?;
                github_tasks::record_progress(
                    project,
                    task_id,
                    "Blocked; waiting for the stated dependency or decision.",
                    reason,
                    &references,
                )
                .await
                .err()
            } else {
                None
            };
            let reason = with_references(reason, &references);
            let updated =
                operations::block_task(&store, room_id, participant_id, task_id, &reason)?;
            if let Some(error) = tracking_error.as_ref() {
                store.update(room_id, |room| { room.board.error = Some(format!("Blocker was saved locally, but GitHub tracking needs reconciliation: {error}")); Ok(()) })?;
            }
            Ok(
                json!({"task_id":task_id,"claim":updated.claims.iter().find(|c| c.task_id == task_id),"github_tracking_error":tracking_error}),
            )
        }
        "request_agent" | "request_decision" | "request_review" | "propose_completion" => {
            let (kind, title, body) = match name {
                "request_agent" => (
                    "agent",
                    required_string(args, "title")?,
                    required_string(args, "reason")?,
                ),
                "request_decision" => (
                    "decision",
                    required_string(args, "title")?,
                    required_string(args, "question")?,
                ),
                "request_review" => (
                    "review",
                    required_string(args, "title")?,
                    required_string(args, "instructions")?,
                ),
                _ => (
                    "completion",
                    required_string(args, "title")?,
                    required_string(args, "summary")?,
                ),
            };
            let string = |field: &str| args.get(field).and_then(Value::as_str).map(str::to_owned);
            let input = CreateRequestInput {
                kind: kind.into(),
                title: title.into(),
                body: body.into(),
                evidence: string("evidence"),
                task_id: string("task_id"),
                reviewer_id: string("reviewer_id"),
                brief: string("brief"),
                proposal: args
                    .get("proposal")
                    .filter(|v| !v.is_null())
                    .map(|v| {
                        serde_json::from_value(v.clone())
                            .map_err(|e| format!("invalid proposal: {e}"))
                    })
                    .transpose()?,
                options: args
                    .get("options")
                    .filter(|v| !v.is_null())
                    .map(|v| {
                        serde_json::from_value(v.clone())
                            .map_err(|e| format!("invalid options: {e}"))
                    })
                    .transpose()?
                    .unwrap_or_default(),
            };
            let updated = governance::create_request(&store, room_id, participant_id, input)?;
            Ok(
                json!({"request": updated.requests.iter().rev().find(|r| r.requester_id == participant_id && r.kind == kind && r.title == title)}),
            )
        }
        "respond_request" => {
            let verdict = required_string(args, "verdict")?;
            if !matches!(verdict, "approved" | "changes_requested") {
                return Err("verdict must be approved or changes_requested".into());
            }
            let request_id = required_string(args, "request_id")?;
            let response = required_string(args, "response")?;
            let updated = governance::respond_request(
                &store,
                room_id,
                participant_id,
                request_id,
                verdict == "approved",
                response,
            )?;
            Ok(json!({"request": updated.requests.iter().find(|r| r.id == request_id)}))
        }
        "release_task" => {
            operations::active_peer(&room, participant_id)?;
            if peer.turn_limit_reached()
                && peer
                    .pending_delivery
                    .as_ref()
                    .is_none_or(|delivery| !matches!(delivery.state.as_str(), "prepared" | "sent"))
            {
                return Err("participant turn budget is exhausted".into());
            }
            let task_id = required_string(args, "task_id")?;
            let claim = room
                .claims
                .iter()
                .find(|c| c.task_id == task_id && c.participant_id == participant_id)
                .ok_or("this participant does not own the task")?;
            if claim.state == "released" {
                return Ok(json!({"task_id": task_id, "released": true}));
            }
            if claim.state == "done" {
                return Err("completed task cannot be released".into());
            }
            let reason = required_string(args, "reason")?;
            if reason.len() > 4000 {
                return Err("release reason exceeds 4000 bytes".into());
            }
            let updated =
                operations::release_own_claim(&store, room_id, participant_id, task_id, reason)
                    .await?;
            Ok(
                json!({"task_id": task_id, "claim": updated.claims.iter().find(|c| c.task_id == task_id)}),
            )
        }
        _ => Err(format!("unknown room tool: {name}")),
    }
}

fn snapshot(room: &Room, peer: &Participant) -> Value {
    let mut messages = room
        .messages
        .iter()
        .rev()
        .filter(|message| {
            (message.sidechat_id.is_none()
                || message.sidechat_id.as_ref().is_some_and(|id| {
                    room.sidechats
                        .iter()
                        .any(|s| &s.id == id && s.participant_ids.contains(&peer.id))
                }))
                && message.targets(&peer.id)
        })
        .take(100)
        .collect::<Vec<_>>();
    messages.reverse();
    json!({
        "room": {"id":room.id,"title":room.title,"objective":room.objective,"repository":room.repository,"paused":room.paused,"archived":room.archived,"project":room.project,"board":room.board,"starting_conversation":room.origin,"sidechats":room.sidechats.iter().filter(|s|s.participant_ids.contains(&peer.id)).collect::<Vec<_>>()},
        "participant": {"id":peer.id,"name":peer.name,"provider":peer.provider,"paused":peer.paused,"state":peer.state,"wake_count":peer.wake_count,"max_turns":peer.max_turns,"branch":peer.branch,"last_error":peer.last_error,"pending_delivery":peer.pending_delivery.as_ref().map(|delivery| json!({"id":delivery.id,"reason":delivery.reason,"state":delivery.state,"created_at":delivery.created_at,"task_id":delivery.task_id,"timer_id":delivery.timer_id}))},
        "participants":room.participants.iter().map(|p| json!({"id":p.id,"name":p.name,"provider":p.provider,"paused":p.paused,"state":p.state,"branch":p.branch})).collect::<Vec<_>>(),
        "messages":messages,
        "active_sidechat_id":peer.active_sidechat_id,
        "claims":room.claims,
        "timers":room.timers,
        "requests":room.requests,
    })
}

fn tool_definitions() -> Value {
    let identity = json!({"room_id":{"type":"string","description":"Must match this scoped room."},"participant_id":{"type":"string","description":"Must match this scoped participant."}});
    let make = |name: &str, description: &str, required: &[&str], extra: Value| {
        let mut properties = identity.as_object().unwrap().clone();
        if let Some(fields) = extra.as_object() {
            properties.extend(fields.clone());
        }
        let mut all_required = vec!["room_id", "participant_id"];
        all_required.extend_from_slice(required);
        json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":all_required,"additionalProperties":false}})
    };
    json!([
        make("snapshot","Read the scoped room snapshot, board, participants, visible shared messages, and claims.",&[],json!({})),
        make("create_sidechat","Create a sidechat from a visible room message, including a message in another sidechat; this never wakes participants.",&["source_message_id","title"],json!({"source_message_id":{"type":"string"},"title":{"type":"string"},"participant_ids":{"type":"array","items":{"type":"string"}}})),
        make("read_sidechat","Read a sidechat, its visible source message, and its messages.",&["sidechat_id"],json!({"sidechat_id":{"type":"string"}})),
        make("post_message","Post to the main room or a sidechat; omitted sidechat_id uses the active sidechat. Use target_participant_id, @Agent name, or explicit @everyone to address peers and wake automatically dormant recipients. Ordinary unaddressed progress broadcasts do not wake dormant peers; explicit human pauses and provider waits are respected.",&["body"],json!({"body":{"type":"string"},"target_participant_id":{"type":["string","null"]},"sidechat_id":{"type":["string","null"],"description":"Omit to use your active sidechat; explicit null posts to the main conversation."}})),
        make("read_task","Read a task that belongs to the room's GitHub Project.",&["task_id"],json!({"task_id":{"type":"string"}})),
        make("create_task","Create an idempotent repository Issue linked to this room's GitHub Project. The detailed body must include nonempty ## Outcome, ## Work, ## Acceptance criteria, ## Progress, and ## References sections; use 'None yet' for references still pending. Set assigned_agent and priority when known and available.",&["title","body"],json!({"title":{"type":"string"},"body":{"type":"string","minLength":160},"assigned_agent":{"type":"string"},"priority":{"type":"string"}})),
        make("update_task","Record an idempotent Issue progress update with evidence and commit, PR, assignment, or other references. Requires your active claim.",&["task_id","progress","evidence"],json!({"task_id":{"type":"string"},"progress":{"type":"string"},"evidence":{"type":"string"},"references":{"type":"array","items":{"type":"string"},"maxItems":30}})),
        make("convert_draft_task","After claiming a visible Project draft, enrich it with a detailed body and convert it to a repository Issue while preserving its Project item. The body needs the same five sections as create_task.",&["task_id","body"],json!({"task_id":{"type":"string"},"body":{"type":"string","minLength":160}})),
        make("claim_task","Atomically claim an eligible room board task and update GitHub.",&["task_id"],json!({"task_id":{"type":"string"}})),
        make("finish_task","Mark your claimed task complete with summary, evidence, and available commit, PR, or other references.",&["task_id","summary","evidence"],json!({"task_id":{"type":"string"},"summary":{"type":"string"},"evidence":{"type":"string"},"references":{"type":"array","items":{"type":"string"},"maxItems":30}})),
        make("block_task","Record the blocker on its repository Issue and pause the participant, with available references.",&["task_id","reason"],json!({"task_id":{"type":"string"},"reason":{"type":"string"},"references":{"type":"array","items":{"type":"string"},"maxItems":30}})),
        make("request_agent","Request a new local peer for HUMAN approval. This never creates a peer or runs a model. Exact retries return the same request.",&["title","reason","brief","proposal"],json!({"title":{"type":"string"},"reason":{"type":"string"},"brief":{"type":"string"},"proposal":{"type":"object","properties":{"name":{"type":"string"},"provider":{"enum":["codex","claude"]},"model":{"type":["string","null"]},"effort":{"type":["string","null"]},"use_worktree":{"type":"boolean"},"max_turns":{"type":"integer","minimum":0,"maximum":200,"description":"Optional total room turn limit; 0 disables it. Separate from each timer delivery limit."}},"required":["name","provider","use_worktree","max_turns"],"additionalProperties":false}})),
        make("request_decision","Ask the human a durable question with optional choices and evidence. The answer wakes the requester when eligible.",&["title","question"],json!({"title":{"type":"string"},"question":{"type":"string"},"options":{"type":"array","items":{"type":"string"},"maxItems":5},"evidence":{"type":["string","null"]}})),
        make("request_review","Ask another room peer to review evidence. The requester cannot approve their own review.",&["title","instructions","evidence","reviewer_id"],json!({"title":{"type":"string"},"instructions":{"type":"string"},"evidence":{"type":"string"},"reviewer_id":{"type":"string"},"task_id":{"type":["string","null"]}})),
        make("propose_completion","Propose final room completion for independent peer verification, then human acceptance. The canonical board must be fresh and complete. This does not close the room.",&["title","summary","evidence","reviewer_id"],json!({"title":{"type":"string"},"summary":{"type":"string"},"evidence":{"type":"string"},"reviewer_id":{"type":"string"}})),
        make("respond_request","Respond only to a review or completion assigned to you, with concrete review evidence. Only the human can answer decisions, add peers or accept final completion.",&["request_id","verdict","response"],json!({"request_id":{"type":"string"},"verdict":{"enum":["approved","changes_requested"]},"response":{"type":"string"}})),
        make("release_task","Release only your own unfinished task back to the canonical board with a reason. Uncertain writes keep ownership until reconciled.",&["task_id","reason"],json!({"task_id":{"type":"string"},"reason":{"type":"string"}}))
    ])
}

/// Finds the room and participant that own an existing provider run ID.
pub fn binding_for_run(run_id: &str) -> Result<Option<(Room, Participant)>, String> {
    let store = open_store(&crate::storage::data_dir())?;
    for room in store.list()? {
        if let Some(peer) = room
            .participants
            .iter()
            .find(|peer| peer.run_id == run_id)
            .cloned()
        {
            return Ok(Some((room, peer)));
        }
    }
    Ok(None)
}

/// Writes the isolated stdio server config for a run, using mode 0600 on Unix.
pub fn config_for_run(run_id: &str) -> Result<Option<PathBuf>, String> {
    let Some((room, peer)) = binding_for_run(run_id)? else {
        return Ok(None);
    };
    let data_dir = crate::storage::data_dir();
    let executable =
        std::env::current_exe().map_err(|e| format!("resolve OpenCovibe executable: {e}"))?;
    write_room_config(&data_dir, &room, &peer, &executable).map(Some)
}

fn write_room_config(
    data_dir: &Path,
    room: &Room,
    peer: &Participant,
    executable: &Path,
) -> Result<PathBuf, String> {
    if !data_dir.is_absolute() || !executable.is_absolute() {
        return Err("room MCP configuration requires absolute paths".into());
    }
    let directory = data_dir.join("room-mcp");
    crate::storage::ensure_dir(&directory).map_err(|e| e.to_string())?;
    let path = directory.join(format!("{}-{}.json", room.id, peer.id));
    let document = json!({"mcpServers":{"room":{"command":executable,"args":["--room-mcp","--data-dir",data_dir,"--room-id",room.id,"--participant-id",peer.id]}}});
    let bytes = serde_json::to_vec_pretty(&document).map_err(|e| e.to_string())?;
    let temporary = path.with_extension(format!("json.{}.tmp", uuid::Uuid::new_v4()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|e| format!("create MCP config: {e}"))?;
    file.write_all(&bytes)
        .map_err(|e| format!("write MCP config: {e}"))?;
    file.sync_all()
        .map_err(|e| format!("sync MCP config: {e}"))?;
    #[cfg(windows)]
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("replace MCP config: {e}"))?;
    }
    std::fs::rename(&temporary, &path).map_err(|e| format!("install MCP config: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
    }
    Ok(path)
}

/// Returns Codex CLI `-c` overrides for the scoped MCP server.
pub fn codex_args_for_run(run_id: &str) -> Result<Option<Vec<String>>, String> {
    let Some(config_path) = config_for_run(run_id)? else {
        return Ok(None);
    };
    let config: Value =
        serde_json::from_slice(&std::fs::read(&config_path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    codex_overrides(&config["mcpServers"]["room"]).map(Some)
}

fn codex_overrides(server: &Value) -> Result<Vec<String>, String> {
    let command = server["command"]
        .as_str()
        .ok_or("room MCP config has no command")?;
    let args = server["args"]
        .as_array()
        .ok_or("room MCP config has no args")?;
    let quote = |value: &str| toml::Value::String(value.to_owned()).to_string();
    let quoted_args = args
        .iter()
        .map(|arg| {
            arg.as_str()
                .map(quote)
                .ok_or("room MCP argument is not a string".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut overrides = vec![
        "-c".into(),
        format!("mcp_servers.room.command={}", quote(command)),
        "-c".into(),
        format!("mcp_servers.room.args=[{}]", quoted_args.join(", ")),
        "-c".into(),
        "mcp_servers.room.enabled=true".into(),
        "-c".into(),
        "mcp_servers.room.required=true".into(),
        "-c".into(),
        "mcp_servers.room.startup_timeout_sec=20".into(),
        "-c".into(),
        "mcp_servers.room.tool_timeout_sec=120".into(),
    ];
    for tool in ROOM_TOOL_NAMES {
        overrides.extend([
            "-c".into(),
            format!("mcp_servers.room.tools.{tool}.approval_mode=\"approve\""),
        ]);
    }
    Ok(overrides)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_approval_is_limited_to_scoped_room_tools() {
        let args = codex_overrides(&json!({"command":"/tmp/Local App","args":["--room-mcp","--room-id","room","--participant-id","peer"]})).unwrap();
        for pair in args.chunks_exact(2) {
            assert_eq!(pair[0], "-c");
            assert!(pair[1].starts_with("mcp_servers.room."));
        }
        let approvals: Vec<_> = args
            .iter()
            .filter(|a| a.contains("approval_mode="))
            .collect();
        assert_eq!(approvals.len(), ROOM_TOOL_NAMES.len());
        assert!(approvals.iter().all(|a| a.ends_with("=\"approve\"")));
        assert!(!args
            .iter()
            .any(|a| a.starts_with("approval_policy=") || a.starts_with("sandbox_mode=")));
    }

    #[tokio::test]
    async fn sidechat_tools_enforce_membership_and_return_visible_context() {
        let temp = tempfile::tempdir().unwrap();
        let data_dir = temp.path().join("data");
        let store = RoomStore::open(&data_dir.join(DB_NAME)).unwrap();
        let room = store
            .create(crate::rooms::models::CreateRoomInput {
                title: "Sidechat scope".into(),
                objective: "Keep branches private".into(),
                repo_path: temp.path().display().to_string(),
                repository: "owner/repo".into(),
                create_project: false,
            })
            .unwrap();
        store
            .update(&room.id, |r| {
                r.paused = false;
                for id in ["a", "b", "c"] {
                    r.participants.push(Participant {
                        id: id.into(),
                        name: id.into(),
                        paused: false,
                        ..Default::default()
                    });
                }
                Ok(())
            })
            .unwrap();
        let source = store
            .append_message(&room.id, "Human", "Shared source".into(), None, None, None)
            .unwrap()
            .messages
            .last()
            .unwrap()
            .id
            .clone();
        let created = call_tool(
            &data_dir,
            &room.id,
            "a",
            "create_sidechat",
            &json!({"source_message_id":source,"title":"Small group","participant_ids":["a","b"]}),
        )
        .await
        .unwrap();
        let id = created["sidechat"]["id"].as_str().unwrap().to_owned();
        store
            .append_message_in_sidechat(
                &room.id,
                "a",
                "Private detail".into(),
                Some("a".into()),
                None,
                None,
                Some(id.clone()),
            )
            .unwrap();
        let read = call_tool(
            &data_dir,
            &room.id,
            "b",
            "read_sidechat",
            &json!({"sidechat_id":id}),
        )
        .await
        .unwrap();
        assert_eq!(read["source"]["body"], "Shared source");
        assert_eq!(read["messages"][0]["body"], "Private detail");
        assert!(call_tool(
            &data_dir,
            &room.id,
            "c",
            "read_sidechat",
            &json!({"sidechat_id":id})
        )
        .await
        .unwrap_err()
        .contains("not a member"));
        let source_message = store
            .get(&room.id)
            .unwrap()
            .messages
            .last()
            .unwrap()
            .id
            .clone();
        let nested = call_tool(
            &data_dir,
            &room.id,
            "a",
            "create_sidechat",
            &json!({"source_message_id":source_message,"title":"Nested","participant_ids":["a"]}),
        )
        .await
        .unwrap();
        assert_eq!(nested["sidechat"]["participant_ids"], json!(["a"]));
        let after_create = store.get(&room.id).unwrap();
        assert!(after_create
            .participants
            .iter()
            .all(|p| p.pending_delivery.is_none()));
        drop(store);
        let reopened = RoomStore::open(&data_dir.join(DB_NAME)).unwrap();
        let persisted = reopened.get(&room.id).unwrap();
        assert_eq!(persisted.sidechats.len(), 2);
        assert_eq!(persisted.sidechats[1].participant_ids, vec!["a"]);
    }

    #[tokio::test]
    async fn bounded_line_reader_accepts_json_lines_and_rejects_oversize() {
        let mut input = BufReader::new(&b"{\"ok\":true}\r\n"[..]);
        assert_eq!(
            read_bounded_line(&mut input).await.unwrap().unwrap(),
            b"{\"ok\":true}"
        );
        let oversized = vec![b'x'; MAX_REQUEST_BYTES + 1];
        let mut input = BufReader::new(oversized.as_slice());
        assert!(read_bounded_line(&mut input)
            .await
            .unwrap_err()
            .contains("1 MiB"));
    }

    #[tokio::test]
    async fn paused_participant_cannot_write_without_network_access() {
        use crate::rooms::models::CreateRoomInput;

        let temp = tempfile::tempdir().unwrap();
        let data_dir = temp.path().join("data");
        let store = RoomStore::open(&data_dir.join(DB_NAME)).unwrap();
        let room = store
            .create(CreateRoomInput {
                title: "Scoped test".into(),
                objective: "Test write authorization".into(),
                repo_path: temp.path().to_string_lossy().into_owned(),
                repository: "owner/repo".into(),
                create_project: false,
            })
            .unwrap();
        let peer = Participant {
            id: "peer-id".into(),
            run_id: "run-id".into(),
            paused: true,
            ..Default::default()
        };
        store
            .update(&room.id, |room| {
                room.paused = false;
                room.participants.push(peer);
                Ok(())
            })
            .unwrap();
        drop(store);

        let result = call_tool(
            &data_dir,
            &room.id,
            "peer-id",
            "post_message",
            &json!({"body":"must be rejected"}),
        )
        .await;
        assert!(result.unwrap_err().contains("participant is paused"));
        let reopened = RoomStore::open(&data_dir.join(DB_NAME)).unwrap();
        assert!(reopened.get(&room.id).unwrap().messages.is_empty());
    }

    #[tokio::test]
    async fn governance_tools_are_scoped_and_cannot_perform_human_approval() {
        let temp = tempfile::tempdir().unwrap();
        let data_dir = temp.path().join("data");
        let store = RoomStore::open(&data_dir.join(DB_NAME)).unwrap();
        let room = store
            .create(crate::rooms::models::CreateRoomInput {
                title: "Governance scope".into(),
                objective: "Test authority".into(),
                repo_path: temp.path().display().to_string(),
                repository: "owner/repo".into(),
                create_project: false,
            })
            .unwrap();
        store
            .update(&room.id, |room| {
                room.paused = false;
                room.participants.push(Participant {
                    id: "peer".into(),
                    name: "Peer".into(),
                    paused: false,
                    max_turns: 2,
                    ..Default::default()
                });
                Ok(())
            })
            .unwrap();
        let decision = call_tool(&data_dir, &room.id, "peer", "request_decision", &json!({"title":"Which approach?","question":"Choose for this test.","options":["A","B"]})).await.unwrap();
        let request_id = decision["request"]["id"].as_str().unwrap();
        let peer_answer = call_tool(
            &data_dir,
            &room.id,
            "peer",
            "respond_request",
            &json!({"request_id":request_id,"verdict":"approved","response":"I choose A"}),
        )
        .await
        .unwrap_err();
        assert!(peer_answer.contains("only review and completion"));
        for name in ["approve_room_agent", "resolve_room_request"] {
            assert!(call_tool(&data_dir, &room.id, "peer", name, &json!({}))
                .await
                .unwrap_err()
                .contains("unknown room tool"));
            assert!(!ROOM_TOOL_NAMES.contains(&name));
        }
        store
            .update(&room.id, |room| {
                room.participants[0].paused = true;
                Ok(())
            })
            .unwrap();
        for name in [
            "request_decision",
            "request_agent",
            "request_review",
            "propose_completion",
            "respond_request",
            "release_task",
        ] {
            let result = call_tool(&data_dir, &room.id, "peer", name, &json!({"title":"No write","question":"Paused","reason":"Paused","instructions":"Paused","summary":"Paused","brief":"Paused","reviewer_id":"peer","verdict":"approved","request_id":request_id,"response":"Paused","task_id":"none"})).await;
            assert!(
                result.is_err(),
                "{name} unexpectedly authorized a paused peer"
            );
        }
        let saved = store.get(&room.id).unwrap();
        assert_eq!(saved.requests.len(), 1);
        assert_eq!(saved.requests[0].status, "pending");
    }

    #[tokio::test]
    async fn initialize_and_tools_list_advertise_the_room_protocol() {
        let temp = tempfile::tempdir().unwrap();
        let initialized = dispatch(temp.path(), "room", "peer", &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}})).await.unwrap();
        assert_eq!(initialized["protocolVersion"], PROTOCOL_VERSION);
        let listed = dispatch(
            temp.path(),
            "room",
            "peer",
            &json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        )
        .await
        .unwrap();
        let names = listed["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            [
                "snapshot",
                "create_sidechat",
                "read_sidechat",
                "post_message",
                "read_task",
                "create_task",
                "update_task",
                "convert_draft_task",
                "claim_task",
                "finish_task",
                "block_task",
                "request_agent",
                "request_decision",
                "request_review",
                "propose_completion",
                "respond_request",
                "release_task"
            ]
        );
    }

    #[tokio::test]
    async fn tool_calls_reject_room_or_participant_scope_mismatches() {
        let temp = tempfile::tempdir().unwrap();
        let request = json!({
            "jsonrpc":"2.0",
            "id":7,
            "method":"tools/call",
            "params":{"name":"snapshot","arguments":{"room_id":"other-room","participant_id":"peer"}}
        });
        let response = dispatch(temp.path(), "room", "peer", &request)
            .await
            .unwrap();
        assert_eq!(response["isError"], true);
        assert!(response["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("does not match"));
    }

    #[test]
    fn scoped_config_is_mode_600_and_has_exact_arguments() {
        let temp = tempfile::tempdir().unwrap();
        let data_dir = temp.path().join("data");
        let binary = temp.path().join("OpenCovibe");
        std::fs::write(&binary, b"").unwrap();
        let room = Room {
            id: "room-id".into(),
            title: "Room".into(),
            objective: "Objective".into(),
            repo_path: temp.path().display().to_string(),
            repository: "owner/repo".into(),
            created_at: "now".into(),
            updated_at: "now".into(),
            paused: false,
            project: None,
            project_stage: Default::default(),
            board: Default::default(),
            participants: vec![],
            messages: vec![],
            sidechats: vec![],
            timers: vec![],
            claims: vec![],
            requests: vec![],
            auto_continue: true,
            max_concurrent: 3,
            archived: false,
            runtime_error: None,
            origin: None,
        };
        let peer = Participant {
            id: "peer-id".into(),
            run_id: "run".into(),
            ..Default::default()
        };
        let path = write_room_config(&data_dir, &room, &peer, &binary).unwrap();
        let value: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            value["mcpServers"]["room"]["command"],
            binary.to_string_lossy().as_ref()
        );
        assert_eq!(value["mcpServers"]["room"]["args"][0], "--room-mcp");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
}

use super::{
    github, github_tasks,
    models::{Participant, Room},
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
        "read_task" => {
            let project = room.project.as_ref().ok_or("room has no GitHub Project")?;
            let task_id = required_string(args, "task_id")?;
            if !room.board.items.iter().any(|item| item.id == task_id) {
                return Err("task is not present on this room's board".into());
            }
            github_tasks::read_task(project, task_id).await
        }
        "post_message" => {
            operations::active_peer(&room, participant_id)?;
            let body = required_string(args, "body")?;
            let target_id = args
                .get("target_participant_id")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let updated = store.append_message(
                room_id,
                &peer.name,
                body.to_owned(),
                Some(participant_id.to_owned()),
                target_id,
                None,
            )?;
            Ok(json!({"message_posted":true,"message_count":updated.messages.len()}))
        }
        "create_task" => {
            operations::active_peer(&room, participant_id)?;
            let project = room.project.as_ref().ok_or("room has no GitHub Project")?;
            let title = required_string(args, "title")?;
            let body = required_string(args, "body")?;
            if title.chars().count() > 200 || body.len() > 32_000 {
                return Err(
                    "task title must be at most 200 characters and body at most 32000 bytes".into(),
                );
            }
            let first_attempt = store.begin_task_creation(room_id, title, body)?;
            let existing_id = store.created_task_id(room_id, title)?;
            let task_id = if let Some(id) = existing_id {
                github_tasks::read_task(project, &id).await?;
                id
            } else if !first_attempt {
                let board = github::read_board(&project.id).await?;
                let matching = board
                    .items
                    .iter()
                    .filter(|item| item.title == title)
                    .collect::<Vec<_>>();
                if matching.len() != 1 {
                    return Err("Previous task creation is unconfirmed or its title is ambiguous. No duplicate was created. Reconcile the GitHub Project before trying a different task title.".into());
                }
                let id = &matching[0].id;
                let existing = github_tasks::read_task(project, id).await?;
                if existing["body"].as_str() != Some(body) {
                    return Err("The unconfirmed task's body changed. Inspect the existing task before resolving its creation intent; no replacement was created.".into());
                }
                id.clone()
            } else {
                github_tasks::create_task(project, title, body).await?
            };
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
            let summary = required_string(args, "summary")?;
            let evidence = required_string(args, "evidence")?;
            let updated = operations::finish_task(
                &store,
                room_id,
                participant_id,
                task_id,
                summary,
                evidence,
            )
            .await?;
            Ok(
                json!({"task_id":task_id,"claim":updated.claims.iter().find(|c| c.task_id == task_id)}),
            )
        }
        "block_task" => {
            operations::active_peer(&room, participant_id)?;
            let task_id = required_string(args, "task_id")?;
            let reason = required_string(args, "reason")?;
            let updated = operations::block_task(&store, room_id, participant_id, task_id, reason)?;
            Ok(
                json!({"task_id":task_id,"claim":updated.claims.iter().find(|c| c.task_id == task_id)}),
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
            message
                .target_participant_id
                .as_deref()
                .is_none_or(|target| target == peer.id)
        })
        .take(100)
        .collect::<Vec<_>>();
    messages.reverse();
    json!({
        "room": {"id":room.id,"title":room.title,"objective":room.objective,"repository":room.repository,"paused":room.paused,"archived":room.archived,"project":room.project,"board":room.board},
        "participant": {"id":peer.id,"name":peer.name,"provider":peer.provider,"paused":peer.paused,"state":peer.state,"wake_count":peer.wake_count,"max_turns":peer.max_turns,"branch":peer.branch,"last_error":peer.last_error,"pending_delivery":peer.pending_delivery.as_ref().map(|delivery| json!({"id":delivery.id,"reason":delivery.reason,"state":delivery.state,"created_at":delivery.created_at,"task_id":delivery.task_id,"timer_id":delivery.timer_id}))},
        "participants":room.participants.iter().map(|p| json!({"id":p.id,"name":p.name,"provider":p.provider,"paused":p.paused,"state":p.state,"branch":p.branch})).collect::<Vec<_>>(),
        "messages":messages,
        "claims":room.claims,
        "timers":room.timers,
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
        make("post_message","Post a room-scoped message, optionally addressed to one participant.",&["body"],json!({"body":{"type":"string"},"target_participant_id":{"type":["string","null"]}})),
        make("read_task","Read a task that belongs to the room's GitHub Project.",&["task_id"],json!({"task_id":{"type":"string"}})),
        make("create_task","Create or find an exact-title draft task on the room's GitHub Project, then refresh its board.",&["title","body"],json!({"title":{"type":"string"},"body":{"type":"string"}})),
        make("claim_task","Atomically claim an eligible room board task and update GitHub.",&["task_id"],json!({"task_id":{"type":"string"}})),
        make("finish_task","Mark a task owned by this participant complete with summary and evidence.",&["task_id","summary","evidence"],json!({"task_id":{"type":"string"},"summary":{"type":"string"},"evidence":{"type":"string"}})),
        make("block_task","Mark a task owned by this participant blocked and pause the participant.",&["task_id","reason"],json!({"task_id":{"type":"string"},"reason":{"type":"string"}}))
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
    for tool in [
        "snapshot",
        "post_message",
        "read_task",
        "create_task",
        "claim_task",
        "finish_task",
        "block_task",
    ] {
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
    fn automatic_approval_is_limited_to_the_seven_scoped_room_tools() {
        let args = codex_overrides(&json!({"command":"/tmp/Local App","args":["--room-mcp","--room-id","room","--participant-id","peer"]})).unwrap();
        for pair in args.chunks_exact(2) {
            assert_eq!(pair[0], "-c");
            assert!(pair[1].starts_with("mcp_servers.room."));
        }
        let approvals: Vec<_> = args
            .iter()
            .filter(|a| a.contains("approval_mode="))
            .collect();
        assert_eq!(approvals.len(), 7);
        assert!(approvals.iter().all(|a| a.ends_with("=\"approve\"")));
        assert!(!args
            .iter()
            .any(|a| a.starts_with("approval_policy=") || a.starts_with("sandbox_mode=")));
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
                "post_message",
                "read_task",
                "create_task",
                "claim_task",
                "finish_task",
                "block_task"
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
            timers: vec![],
            claims: vec![],
            auto_continue: true,
            archived: false,
            runtime_error: None,
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

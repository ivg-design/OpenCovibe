//! Durable external messages addressed to an existing standalone OCV Codex run.
//! The session actor remains the only provider writer. This worker never starts one.
use super::{
    authorized_receipt, current_principal, decode, save_receipt, Principal, Receipt, Reply, Send,
};
use crate::{
    agent::{adapter::ActorSessionMap, session_actor::ActorCommand},
    models::{ConversationRef, ExecutionPath, RunStatus},
    rooms::store::RoomStore,
    storage,
};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

fn eligible(run_id: &str) -> Result<crate::models::RunMeta, String> {
    let meta = storage::runs::get_run(run_id).ok_or("Session unavailable")?;
    if !managed_codex(&meta, crate::rooms::mcp::binding_for_run(run_id)?.is_some()) {
        return Err("Standalone OCV Codex session unavailable".into());
    }
    Ok(meta)
}

fn managed_codex(meta: &crate::models::RunMeta, room_owned: bool) -> bool {
    meta.agent == "codex"
        && meta.resolved_execution_path() == ExecutionPath::SessionActor
        && matches!(
            meta.resolved_conversation_ref(),
            Some(ConversationRef::CodexThread(_))
        )
        && !meta.no_session_persistence
        && !room_owned
}

fn provider_text(text: &str) -> String {
    if text.trim_start().starts_with('/') {
        format!("External message text (literal content):\n{text}")
    } else {
        text.into()
    }
}

pub(super) fn agents(principal: &Principal, rooms: &[crate::rooms::models::Room]) -> Vec<Value> {
    if !principal.all_sessions {
        return Vec::new();
    }
    let owned: std::collections::HashSet<&str> = rooms
        .iter()
        .flat_map(|r| r.participants.iter().map(|p| p.run_id.as_str()))
        .collect();
    let mut agents = Vec::new();
    for meta in storage::runs::list_all_run_metas() {
        if !managed_codex(&meta, owned.contains(meta.id.as_str())) {
            continue;
        }
        let thread_id = match meta.resolved_conversation_ref() {
            Some(ConversationRef::CodexThread(id)) => id,
            _ => continue,
        };
        let blocked = if meta.status == RunStatus::Idle {
            None
        } else {
            Some(format!("Session {}", meta.status))
        };
        agents.push(json!({
            "agent_id":format!("session/{}",meta.id),"target_kind":"session",
            "run_id":meta.id,"name":meta.name.unwrap_or(meta.prompt),"provider":"codex",
            "provider_thread_id":thread_id,"state":meta.status,"blocked_reason":blocked,
            "allowed_actions":if principal.scopes.iter().any(|s|s=="send") { vec!["read","queue"] } else { vec!["read"] }
        }));
    }
    agents
}

pub(super) fn send(
    store: &RoomStore,
    principal: &Principal,
    input: &Send,
    run_id: &str,
) -> Result<Receipt, String> {
    principal.session(&input.conversation_ref)?;
    if uuid::Uuid::parse_str(run_id).is_err()
        || input.mode != "queue"
        || !input.attachments.is_empty()
        || input.text.trim().is_empty()
        || input.text.len() > 32_000
        || input.client_message_id.is_empty()
        || input.client_message_id.len() > 200
    {
        return Err("Invalid standalone session message".into());
    }
    let payload = serde_json::to_string(input).map_err(|e| e.to_string())?;
    // Retry identity is checked before live run state, so a stopped/deleted run can still
    // return its original receipt without accidentally creating a second message.
    {
        let conn = store.connection.lock().map_err(|e| e.to_string())?;
        let old: Option<(String, String)> = conn
            .query_row(
                "SELECT payload,receipt FROM bridge_receipts WHERE principal=?1 AND client_id=?2",
                params![principal.id, input.client_message_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some((saved, receipt)) = old {
            return if saved == payload {
                decode(receipt)
            } else {
                Err("Idempotency conflict: client_message_id payload changed".into())
            };
        }
    }
    let meta = eligible(run_id)?;
    let mut conn = store.connection.lock().map_err(|e| e.to_string())?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let old: Option<(String, String)> = tx
        .query_row(
            "SELECT payload,receipt FROM bridge_receipts WHERE principal=?1 AND client_id=?2",
            params![principal.id, input.client_message_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if let Some((saved, receipt)) = old {
        return if saved == payload {
            decode(receipt)
        } else {
            Err("Idempotency conflict: client_message_id payload changed".into())
        };
    }
    if let Some(reply_id) = &input.reply_to_message_id {
        let prior = authorized_receipt(&tx, principal, reply_id)?;
        if prior.target_kind != "session"
            || prior.target_run_id.as_deref() != Some(run_id)
            || prior.conversation_ref != input.conversation_ref
        {
            return Err("Reply session/conversation mismatch".into());
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    let receipt = Receipt {
        message_id: id.clone(),
        room_id: String::new(),
        target_kind: "session".into(),
        target_run_id: Some(run_id.into()),
        agent_id: input.agent_id.clone(),
        conversation_ref: input.conversation_ref.clone(),
        room_message_id: String::new(),
        state: "queued".into(),
        queued_at: crate::models::now_iso(),
        delivered_at: None,
        running_at: None,
        completed_at: None,
        delivery_id: None,
        provider_turn_id: None,
        run_id: Some(run_id.into()),
        error: None,
        blocked_reason: if meta.status == RunStatus::Idle {
            None
        } else {
            Some(format!("Session {}", meta.status))
        },
    };
    tx.execute(
        "INSERT INTO bridge_receipts VALUES (?1,?2,?3,?4,?5,?6)",
        params![
            id,
            principal.id,
            input.client_message_id,
            payload,
            "",
            serde_json::to_string(&receipt).map_err(|e| e.to_string())?
        ],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO bridge_direct(receipt_id,run_id,phase,cursor,offset) VALUES (?1,?2,'queued',0,0)",
        params![id, run_id],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(receipt)
}

pub fn start(store: Arc<RoomStore>, sessions: ActorSessionMap, cancel: CancellationToken) {
    tokio::spawn(async move {
        // A delivery already handed to a provider before worker restart is never replayed.
        if let Err(e) = recover(&store) {
            log::warn!("[bridge/direct] recovery: {e}");
        }
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(500));
        loop {
            tokio::select! { _=cancel.cancelled()=>break, _=tick.tick()=>{} }
            if let Err(e) = poll(&store, &sessions).await {
                log::warn!("[bridge/direct] poll: {e}");
            }
        }
    });
}

fn recover(store: &RoomStore) -> Result<(), String> {
    let ids: Vec<(String, String, i64)> = {
        let conn = store.connection.lock().map_err(|e| e.to_string())?;
        let mut q=conn.prepare("SELECT receipt_id,run_id,cursor FROM bridge_direct WHERE phase IN ('dispatching','active')").map_err(|e|e.to_string())?;
        let rows = q
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        rows
    };
    for (id, run_id, cursor) in ids {
        project_events(store, &id, &run_id, cursor)?;
        let conn = store.connection.lock().map_err(|e| e.to_string())?;
        let raw: String = conn
            .query_row(
                "SELECT receipt FROM bridge_receipts WHERE id=?1",
                [&id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let mut receipt: Receipt = decode(raw)?;
        if super::terminal(&receipt.state) {
            continue;
        }
        receipt.state = "ambiguous".into();
        receipt.error = Some(
            "Delivery interrupted; inspect session before retrying with a new message ID".into(),
        );
        receipt.completed_at = Some(crate::models::now_iso());
        save_receipt(&conn, &receipt)?;
        conn.execute(
            "UPDATE bridge_direct SET phase='terminal' WHERE receipt_id=?1",
            [&id],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

async fn poll(store: &Arc<RoomStore>, sessions: &ActorSessionMap) -> Result<(), String> {
    let rows: Vec<(String, String, String, i64)> = {
        let conn = store.connection.lock().map_err(|e| e.to_string())?;
        let mut q=conn.prepare("SELECT receipt_id,run_id,phase,cursor FROM bridge_direct WHERE phase IN ('queued','dispatching','active') ORDER BY rowid").map_err(|e|e.to_string())?;
        let rows = q
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        rows
    };
    for (id, run_id, phase, cursor) in rows {
        if phase == "queued" {
            try_dispatch(store, sessions, &id, &run_id).await?;
        } else {
            project_events(store, &id, &run_id, cursor)?;
        }
    }
    Ok(())
}

async fn try_dispatch(
    store: &Arc<RoomStore>,
    sessions: &ActorSessionMap,
    id: &str,
    run_id: &str,
) -> Result<(), String> {
    let (principal_id, raw_payload): (String, String) = {
        let conn = store.connection.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT principal,payload FROM bridge_receipts WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| e.to_string())?
    };
    let input: Send = decode(raw_payload)?;
    let principal = match current_principal(&principal_id) {
        Ok(p) => p,
        Err(_) => return set_queued_reason(store, id, Some("Bridge grant revoked")),
    };
    if principal.scope("send").is_err() || principal.session(&input.conversation_ref).is_err() {
        return set_queued_reason(store, id, Some("Bridge send grant revoked"));
    }
    let meta = match eligible(run_id) {
        Ok(m) => m,
        Err(_) => return set_queued_reason(store, id, Some("Session unavailable or room-owned")),
    };
    if meta.status != RunStatus::Idle {
        return set_queued_reason(store, id, Some("Session is not idle"));
    }
    let tx = match sessions.lock().await.get(run_id) {
        Some(h) => h.cmd_tx.clone(),
        None => {
            return set_queued_reason(
                store,
                id,
                Some("Session disconnected; resume it in OpenCovibe"),
            )
        }
    };
    {
        let conn = store.connection.lock().map_err(|e| e.to_string())?;
        let busy:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM bridge_direct WHERE run_id=?1 AND receipt_id<>?2 AND phase IN ('dispatching','active'))",params![run_id,id],|r|r.get(0)).map_err(|e|e.to_string())?;
        if busy {
            drop(conn);
            return set_queued_reason(store, id, Some("Previous external turn is active"));
        }
        let cursor = storage::events::next_seq(run_id).saturating_sub(1);
        let offset = std::fs::metadata(storage::run_dir(run_id).join("events.jsonl"))
            .map(|m| m.len())
            .unwrap_or(0);
        conn.execute("UPDATE bridge_direct SET phase='dispatching',cursor=?1,offset=?2 WHERE receipt_id=?3 AND phase='queued'",params![cursor,offset,id]).map_err(|e|e.to_string())?;
    }
    set_queued_reason(store, id, None)?;
    let store = Arc::clone(store);
    let id = id.to_owned();
    let run_id = run_id.to_owned();
    tokio::spawn(async move {
        let still_authorized = current_principal(&principal_id).is_ok_and(|current| {
            current.scope("send").is_ok() && current.session(&input.conversation_ref).is_ok()
        });
        if !still_authorized || eligible(&run_id).is_err() {
            if let Err(e) = mark_terminal(
                &store,
                &id,
                "cancelled",
                "Grant or session ownership changed before delivery",
            ) {
                log::warn!("[bridge/direct] cancel: {e}");
            }
            return;
        }
        let (reply, result) = tokio::sync::oneshot::channel();
        let sent = tx
            .send(ActorCommand::BridgeMessage {
                text: provider_text(&input.text),
                message_id: id.clone(),
                reply,
            })
            .await;
        let answer = if sent.is_ok() {
            result
                .await
                .map_err(|_| "Actor dropped reply".to_owned())
                .and_then(|v| v)
        } else {
            Err("Actor closed".into())
        };
        if let Err(error) = answer {
            if let Err(e) = mark_terminal(&store, &id, "ambiguous", &error) {
                log::warn!("[bridge/direct] mark ambiguous: {e}");
            }
        } else {
            let cursor = store
                .connection
                .lock()
                .ok()
                .and_then(|conn| {
                    conn.query_row(
                        "SELECT cursor FROM bridge_direct WHERE receipt_id=?1",
                        [&id],
                        |r| r.get(0),
                    )
                    .ok()
                })
                .unwrap_or(0);
            if let Err(e) = project_events(&store, &id, &run_id, cursor) {
                log::warn!("[bridge/direct] project: {e}");
            }
        }
    });
    Ok(())
}

fn set_queued_reason(store: &RoomStore, id: &str, reason: Option<&str>) -> Result<(), String> {
    let conn = store.connection.lock().map_err(|e| e.to_string())?;
    let raw: String = conn
        .query_row(
            "SELECT receipt FROM bridge_receipts WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let mut receipt: Receipt = decode(raw)?;
    if receipt.state == "queued" && receipt.blocked_reason.as_deref() != reason {
        receipt.blocked_reason = reason.map(str::to_owned);
        save_receipt(&conn, &receipt)?;
    }
    Ok(())
}

fn mark_terminal(store: &RoomStore, id: &str, state: &str, error: &str) -> Result<(), String> {
    let conn = store.connection.lock().map_err(|e| e.to_string())?;
    let raw: String = conn
        .query_row(
            "SELECT receipt FROM bridge_receipts WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let mut receipt: Receipt = decode(raw)?;
    if super::terminal(&receipt.state) {
        return Ok(());
    }
    receipt.state = state.into();
    receipt.error = Some(error.into());
    receipt.completed_at = Some(crate::models::now_iso());
    save_receipt(&conn, &receipt)?;
    conn.execute(
        "UPDATE bridge_direct SET phase='terminal' WHERE receipt_id=?1",
        [id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn project_events(store: &RoomStore, id: &str, run_id: &str, cursor: i64) -> Result<(), String> {
    project_events_with(store, id, cursor, |seq, offset| {
        storage::events::list_bus_events_page(run_id, seq, Some(offset))
    })
}

fn project_events_with<F>(
    store: &RoomStore,
    id: &str,
    cursor: i64,
    mut read: F,
) -> Result<(), String>
where
    F: FnMut(u64, u64) -> Result<storage::events::BusEventPage, String>,
{
    let mut seq = cursor.max(0) as u64;
    let mut offset: u64 = store
        .connection
        .lock()
        .map_err(|e| e.to_string())?
        .query_row(
            "SELECT offset FROM bridge_direct WHERE receipt_id=?1",
            [id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    for _ in 0..100 {
        let page = read(seq, offset)?;
        for event in &page.events {
            let event_seq = event["_seq"].as_u64().unwrap_or(seq);
            if event_seq <= seq {
                continue;
            }
            let conn = store.connection.lock().map_err(|e| e.to_string())?;
            let phase: String = conn
                .query_row(
                    "SELECT phase FROM bridge_direct WHERE receipt_id=?1",
                    [id],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            if phase == "terminal" {
                return Ok(());
            }
            let raw: String = conn
                .query_row(
                    "SELECT receipt FROM bridge_receipts WHERE id=?1",
                    [id],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            let mut receipt: Receipt = decode(raw)?;
            let kind = event["type"].as_str().unwrap_or("");
            if phase == "dispatching"
                && kind == "user_message"
                && event["client_uuid"].as_str() == Some(id)
            {
                receipt.state = "running".into();
                receipt.delivered_at = Some(crate::models::now_iso());
                receipt.running_at = receipt.delivered_at.clone();
                receipt.blocked_reason = None;
                save_receipt(&conn, &receipt)?;
                conn.execute(
                    "UPDATE bridge_direct SET phase='active',cursor=?1 WHERE receipt_id=?2",
                    params![event_seq, id],
                )
                .map_err(|e| e.to_string())?;
            } else if phase == "active" {
                if kind == "user_message" && event["client_uuid"].as_str() != Some(id) {
                    receipt.state = "ambiguous".into();
                    receipt.error =
                        Some("Another turn began before this delivery completed".into());
                    receipt.completed_at = Some(crate::models::now_iso());
                    save_receipt(&conn, &receipt)?;
                    conn.execute(
                        "UPDATE bridge_direct SET phase='terminal',cursor=?1 WHERE receipt_id=?2",
                        params![event_seq, id],
                    )
                    .map_err(|e| e.to_string())?;
                } else if matches!(kind, "permission_prompt" | "elicitation_prompt") {
                    receipt.state = "awaiting_input".into();
                    receipt.blocked_reason = Some("Session awaits user input".into());
                    save_receipt(&conn, &receipt)?;
                } else if kind == "message_complete" && event["parent_tool_use_id"].is_null() {
                    if let Some(text) = event["text"].as_str().filter(|s| !s.trim().is_empty()) {
                        let source = event["message_id"]
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| event_seq.to_string());
                        let key = format!("{id}:{source}");
                        let reply = Reply {
                            event_id: format!("evt-{key}"),
                            message_id: source.clone(),
                            reply_to_message_id: id.into(),
                            conversation_ref: receipt.conversation_ref.clone(),
                            agent_id: receipt.agent_id.clone(),
                            sequence: 0,
                            timestamp: event["ts"].as_str().unwrap_or("").into(),
                            status: "visible_reply".into(),
                            text: text.into(),
                            room_message_id: source,
                        };
                        conn.execute("INSERT OR IGNORE INTO bridge_outbox(principal,conversation,event_key,payload) SELECT principal,?1,?2,?3 FROM bridge_receipts WHERE id=?4",params![receipt.conversation_ref,key,serde_json::to_string(&reply).map_err(|e|e.to_string())?,id]).map_err(|e|e.to_string())?;
                        let out_seq: i64 = conn
                            .query_row(
                                "SELECT seq FROM bridge_outbox WHERE event_key=?1",
                                [&key],
                                |r| r.get(0),
                            )
                            .map_err(|e| e.to_string())?;
                        let mut reply = reply;
                        reply.sequence = out_seq;
                        conn.execute(
                            "UPDATE bridge_outbox SET payload=?1 WHERE seq=?2",
                            params![
                                serde_json::to_string(&reply).map_err(|e| e.to_string())?,
                                out_seq
                            ],
                        )
                        .map_err(|e| e.to_string())?;
                    }
                } else if kind == "run_state"
                    && matches!(
                        event["state"].as_str(),
                        Some("idle" | "completed" | "failed" | "stopped")
                    )
                {
                    receipt.state =
                        if matches!(event["state"].as_str(), Some("idle" | "completed")) {
                            "completed"
                        } else {
                            "ambiguous"
                        }
                        .into();
                    receipt.completed_at = Some(crate::models::now_iso());
                    receipt.error = event["error"].as_str().map(str::to_owned);
                    save_receipt(&conn, &receipt)?;
                    conn.execute(
                        "UPDATE bridge_direct SET phase='terminal',cursor=?1 WHERE receipt_id=?2",
                        params![event_seq, id],
                    )
                    .map_err(|e| e.to_string())?;
                }
            }
            conn.execute(
                "UPDATE bridge_direct SET cursor=?1 WHERE receipt_id=?2 AND phase<>'terminal'",
                params![event_seq, id],
            )
            .map_err(|e| e.to_string())?;
            seq = event_seq;
        }
        offset = page.next_offset;
        store
            .connection
            .lock()
            .map_err(|e| e.to_string())?
            .execute(
                "UPDATE bridge_direct SET offset=?1 WHERE receipt_id=?2 AND phase<>'terminal'",
                params![offset, id],
            )
            .map_err(|e| e.to_string())?;
        if !page.has_more {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rooms::bridge::Binding;

    #[test]
    fn leading_slash_is_literal_provider_text() {
        assert_eq!(provider_text("ordinary"), "ordinary");
        assert_eq!(
            provider_text(" /clear"),
            "External message text (literal content):\n /clear"
        );
    }

    #[test]
    fn imported_codex_run_is_eligible_after_ocv_actor_conversion() {
        let meta:crate::models::RunMeta=serde_json::from_value(json!({
            "id":"imported","prompt":"Imported Codex chat","cwd":"/tmp","agent":"codex",
            "status":"idle","started_at":"2026-10-03T00:00:00Z","source":"cli_import",
            "execution_path":"session_actor","conversation_ref":{"kind":"codex_thread","id":"thread-1"}
        })).unwrap();
        assert!(managed_codex(&meta, false));
        assert!(!managed_codex(&meta, true));
        let mut pipe = meta.clone();
        pipe.execution_path = Some(ExecutionPath::PipeExec);
        assert!(!managed_codex(&pipe, false));
    }

    fn fixture(phase: &str) -> (tempfile::TempDir, RoomStore, Principal, Send, Receipt) {
        let dir = tempfile::tempdir().unwrap();
        let store = RoomStore::open(&dir.path().join("rooms.sqlite3")).unwrap();
        let run_id = uuid::Uuid::new_v4().to_string();
        let principal = Principal {
            id: "direct-test".into(),
            token_sha256: String::new(),
            bindings: vec![],
            all_rooms: false,
            all_sessions: true,
            conversations: vec!["dot".into()],
            callback_hosts: vec![],
            scopes: vec!["read".into(), "send".into()],
        };
        let input = Send {
            agent_id: format!("session/{run_id}"),
            text: "hello".into(),
            client_message_id: "client-one".into(),
            conversation_ref: "dot".into(),
            reply_to_message_id: None,
            mode: "queue".into(),
            attachments: vec![],
        };
        let receipt = Receipt {
            message_id: uuid::Uuid::new_v4().to_string(),
            room_id: String::new(),
            target_kind: "session".into(),
            target_run_id: Some(run_id.clone()),
            agent_id: input.agent_id.clone(),
            conversation_ref: "dot".into(),
            room_message_id: String::new(),
            state: "queued".into(),
            queued_at: crate::models::now_iso(),
            delivered_at: None,
            running_at: None,
            completed_at: None,
            delivery_id: None,
            provider_turn_id: None,
            run_id: Some(run_id.clone()),
            error: None,
            blocked_reason: None,
        };
        {
            let conn = store.connection.lock().unwrap();
            conn.execute(
                "INSERT INTO bridge_receipts VALUES (?1,?2,?3,?4,?5,?6)",
                params![
                    receipt.message_id,
                    principal.id,
                    input.client_message_id,
                    serde_json::to_string(&input).unwrap(),
                    "",
                    serde_json::to_string(&receipt).unwrap()
                ],
            )
            .unwrap();
            conn.execute("INSERT INTO bridge_direct(receipt_id,run_id,phase,cursor,offset) VALUES (?1,?2,?3,0,0)",params![receipt.message_id,run_id,phase]).unwrap();
        }
        (dir, store, principal, input, receipt)
    }

    #[test]
    fn direct_receipt_requires_session_grant_and_exact_retry_survives_missing_run() {
        let (_dir, store, principal, input, receipt) = fixture("queued");
        assert_eq!(
            store
                .bridge_get(&principal, &receipt.message_id)
                .unwrap()
                .target_kind,
            "session"
        );
        assert_eq!(
            store.bridge_send(&principal, &input).unwrap().message_id,
            receipt.message_id
        );
        let mut changed = input.clone();
        changed.text = "different".into();
        assert!(store
            .bridge_send(&principal, &changed)
            .unwrap_err()
            .contains("Idempotency conflict"));
        let mut room_only = principal.clone();
        room_only.all_sessions = false;
        room_only.all_rooms = true;
        room_only.bindings = vec![Binding {
            room_id: "some-room".into(),
            conversation_ref: "dot".into(),
        }];
        assert!(store.bridge_get(&room_only, &receipt.message_id).is_err());
    }

    #[test]
    fn interrupted_delivery_becomes_ambiguous_and_is_not_queued_again() {
        let (dir, store, principal, _input, receipt) = fixture("dispatching");
        recover(&store).unwrap();
        assert_eq!(
            store
                .bridge_get(&principal, &receipt.message_id)
                .unwrap()
                .state,
            "ambiguous"
        );
        let reopened = RoomStore::open(&dir.path().join("rooms.sqlite3")).unwrap();
        recover(&reopened).unwrap();
        let conn = reopened.connection.lock().unwrap();
        let phase: String = conn
            .query_row(
                "SELECT phase FROM bridge_direct WHERE receipt_id=?1",
                [&receipt.message_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(phase, "terminal");
    }

    #[test]
    fn tagged_turn_projects_only_its_visible_reply_and_terminal_state() {
        let (_dir, store, principal, _input, receipt) = fixture("dispatching");
        let events = vec![
            json!({"_seq":1,"type":"user_message","client_uuid":"another-ui-turn"}),
            json!({"_seq":2,"type":"message_complete","message_id":"other","text":"unrelated"}),
            json!({"_seq":3,"type":"user_message","client_uuid":receipt.message_id}),
            json!({"_seq":4,"type":"message_complete","message_id":"hidden","text":"secret","parent_tool_use_id":"tool"}),
            json!({"_seq":5,"type":"message_complete","message_id":"answer","text":"target answer","parent_tool_use_id":null,"ts":"2026-10-03T00:00:00Z"}),
            json!({"_seq":6,"type":"permission_prompt"}),
            json!({"_seq":7,"type":"run_state","state":"idle"}),
            json!({"_seq":8,"type":"message_complete","message_id":"late","text":"another turn"}),
        ];
        project_events_with(&store, &receipt.message_id, 0, |_, _| {
            Ok(storage::events::BusEventPage {
                events: events.clone(),
                last_seq: 8,
                has_more: false,
                next_offset: 100,
            })
        })
        .unwrap();
        project_events_with(&store, &receipt.message_id, 0, |_, _| {
            Ok(storage::events::BusEventPage {
                events: events.clone(),
                last_seq: 8,
                has_more: false,
                next_offset: 100,
            })
        })
        .unwrap();
        assert_eq!(
            store
                .bridge_get(&principal, &receipt.message_id)
                .unwrap()
                .state,
            "completed"
        );
        let replies = store.bridge_replies(&principal, "dot", 0, 20).unwrap();
        assert_eq!(replies["replies"].as_array().unwrap().len(), 1);
        assert_eq!(replies["replies"][0]["text"], "target answer");
    }

    #[test]
    fn another_user_turn_before_completion_is_ambiguous() {
        let (_dir, store, principal, _input, receipt) = fixture("dispatching");
        let events = vec![
            json!({"_seq":1,"type":"user_message","client_uuid":receipt.message_id}),
            json!({"_seq":2,"type":"user_message","client_uuid":"another-ui-turn"}),
            json!({"_seq":3,"type":"message_complete","message_id":"other","text":"unrelated"}),
        ];
        project_events_with(&store, &receipt.message_id, 0, |_, _| {
            Ok(storage::events::BusEventPage {
                events: events.clone(),
                last_seq: 3,
                has_more: false,
                next_offset: 100,
            })
        })
        .unwrap();
        assert_eq!(
            store
                .bridge_get(&principal, &receipt.message_id)
                .unwrap()
                .state,
            "ambiguous"
        );
        assert!(
            store.bridge_replies(&principal, "dot", 0, 20).unwrap()["replies"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
}

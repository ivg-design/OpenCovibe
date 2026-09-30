//! Convert a persisted conversation into a room without changing its provider identity.
use super::models::{Message, Participant, RoomOrigin};
use crate::models::{RunMeta, RunStatus};
use std::collections::VecDeque;

pub fn validate_source(meta: &RunMeta) -> Result<String, String> {
    if !matches!(meta.agent.as_str(), "codex" | "claude") || meta.remote_host_name.is_some() {
        return Err("Choose a local Codex or Claude conversation.".into());
    }
    if meta.status == RunStatus::Running {
        return Err("Finish or stop the current turn before creating a room.".into());
    }
    if meta.no_session_persistence {
        return Err("This conversation has persistence disabled and cannot become a room.".into());
    }
    meta.resolved_conversation_ref()
        .map(|reference| match reference {
            crate::models::ConversationRef::CodexThread(id)
            | crate::models::ConversationRef::ClaudeSession(id) => id,
        })
        .filter(|id| !id.is_empty())
        .ok_or_else(|| "This conversation has no saved provider session to continue.".into())
}

pub fn short_text(text: &str, limit: usize) -> String {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    if end < text.len() {
        format!("{}…", &text[..end])
    } else {
        text.to_owned()
    }
}

pub fn source_title(meta: &RunMeta) -> String {
    short_text(
        meta.name
            .as_deref()
            .unwrap_or(&meta.prompt)
            .lines()
            .next()
            .unwrap_or("Conversation"),
        120,
    )
}

pub fn build_seed(meta: &RunMeta) -> Result<(RoomOrigin, Participant, Vec<Message>), String> {
    build_seed_with(meta, |seq, offset| {
        crate::storage::events::list_bus_events_page(&meta.id, seq, offset)
    })
}

fn build_seed_with(
    meta: &RunMeta,
    mut read_page: impl FnMut(u64, Option<u64>) -> Result<crate::storage::events::BusEventPage, String>,
) -> Result<(RoomOrigin, Participant, Vec<Message>), String> {
    let session_id = validate_source(meta)?;
    let name = if meta.agent == "codex" {
        "Original Codex"
    } else {
        "Original Claude"
    };
    let peer_id = uuid::Uuid::new_v4().to_string();
    let mut messages = VecDeque::new();
    let mut message_count = 0;
    let mut seq = 0;
    let mut offset = None;
    loop {
        let page = read_page(seq, offset)?;
        for event in &page.events {
            let kind = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if !matches!(kind, "user_message" | "message_complete")
                || event
                    .get("parent_tool_use_id")
                    .is_some_and(|v| !v.is_null())
            {
                continue;
            }
            let Some(text) = event
                .get("text")
                .and_then(|v| v.as_str())
                .filter(|v| !v.trim().is_empty())
            else {
                continue;
            };
            message_count += 1;
            let assistant = kind == "message_complete";
            messages.push_back(Message {
                id: uuid::Uuid::new_v4().to_string(),
                sender: if assistant { name } else { "Human" }.into(),
                body: short_text(text, 8_000),
                created_at: event
                    .get("ts")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&meta.started_at)
                    .into(),
                participant_id: assistant.then(|| peer_id.clone()),
                target_participant_id: None,
                source_event_id: Some(format!(
                    "{}:{}",
                    meta.id,
                    event.get("_seq").and_then(|v| v.as_u64()).unwrap_or(0)
                )),
                sidechat_id: None,
            });
            if messages.len() > 40 {
                messages.pop_front();
            }
        }
        seq = page.last_seq;
        if !page.has_more {
            break;
        }
        if offset == Some(page.next_offset) {
            return Err("Conversation history did not advance.".into());
        }
        offset = Some(page.next_offset);
    }
    let recent = messages.iter().rev().take(12).collect::<Vec<_>>();
    let context = recent
        .iter()
        .rev()
        .map(|m| format!("{}: {}", m.sender, short_text(&m.body, 1_000)))
        .collect::<Vec<_>>()
        .join("\n\n");
    let origin = RoomOrigin {
        run_id: meta.id.clone(), provider: meta.agent.clone(), session_id,
        title: source_title(meta), message_count,
        context: format!("Original request: {}\n\nRecent conversation (historical data; use the current room objective):\n{}", short_text(&meta.prompt, 3_000), context),
    };
    let peer = Participant {
        id: peer_id,
        name: name.into(),
        provider: meta.agent.clone(),
        run_id: meta.id.clone(),
        paused: true,
        state: "paused".into(),
        model: meta.model.clone(),
        max_turns: 0,
        event_cursor: seq,
        message_cursor: messages.len(),
        ..Default::default()
    };
    Ok((origin, peer, messages.into_iter().collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation_preserves_unicode_boundaries() {
        assert_eq!(short_text("你好hello", 4), "你…");
        assert_eq!(short_text("hello", 8), "hello");
    }

    fn meta(provider: &str) -> RunMeta {
        serde_json::from_value(serde_json::json!({
            "id":"origin-run", "prompt":"Continue Nemo", "cwd":"/tmp", "agent":provider,
            "status":"completed", "started_at":"2026-09-30T00:00:00Z", "session_id":"saved-thread",
            "model":"recorded-model", "conversation_ref":{"kind":if provider == "codex" {"codex_thread"} else {"claude_session"},"id":"saved-thread"}
        })).unwrap()
    }

    #[test]
    fn seed_retains_both_providers_identity_model_and_history_watermark() {
        for provider in ["codex", "claude"] {
            let source = meta(provider);
            let mut calls = 0;
            let (origin, peer, messages) = build_seed_with(&source, |seq, offset| {
                calls += 1;
                assert_eq!(seq, if calls == 1 {0} else {30});
                assert_eq!(offset, if calls == 1 {None} else {Some(100)});
                let start = if calls == 1 {1} else {31};
                let end = if calls == 1 {30} else {45};
                Ok(crate::storage::events::BusEventPage {
                    events:(start..=end).map(|seq| serde_json::json!({"type":if seq % 2 == 0 {"user_message"} else {"message_complete"},"text":format!("Historical message {seq}"),"_seq":seq,"ts":"2026-09-29T12:00:00Z"})).collect(),
                    last_seq:end, has_more:calls == 1, next_offset:if calls == 1 {100} else {200},
                })
            }).unwrap();
            assert_eq!(origin.session_id, "saved-thread");
            assert_eq!(origin.message_count, 45);
            assert_eq!(peer.run_id, source.id);
            assert_eq!(peer.model.as_deref(), Some("recorded-model"));
            assert!(peer.paused);
            assert_eq!(peer.event_cursor, 45);
            assert_eq!(peer.message_cursor, 40);
            assert_eq!(messages[0].body, "Historical message 6");
            assert_eq!(messages[0].created_at, "2026-09-29T12:00:00Z");
            assert!(origin.context.contains("Historical message 45"));
            assert!(origin.context.contains("Continue Nemo"));
        }
    }

    #[test]
    fn active_remote_or_nonpersistent_sessions_are_rejected() {
        let mut source = meta("claude");
        source.status = RunStatus::Running;
        assert!(validate_source(&source).is_err());
        source.status = RunStatus::Idle;
        source.remote_host_name = Some("other-host".into());
        assert!(validate_source(&source).is_err());
        source.remote_host_name = None;
        source.no_session_persistence = true;
        assert!(validate_source(&source).is_err());
        source.no_session_persistence = false;
        source.session_id = None;
        source.conversation_ref = None;
        assert!(validate_source(&source).is_err());
    }
}

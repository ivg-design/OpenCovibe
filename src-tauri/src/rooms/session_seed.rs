//! Convert a persisted conversation into a room without changing its provider identity.
use super::models::{Message, Participant, RoomOrigin};
use crate::models::{RunMeta, RunStatus};
use crate::storage::history::{HistoryEntry, HistoryPage};
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
    validate_source(meta)?;
    // Room context is conversation text, not raw tool output. Use the same
    // bounded projection as the history UI, pinned to one immutable generation.
    let history = crate::storage::history::get_summary(&meta.id, true)?;
    let (origin, mut peer, messages) =
        build_seed_with(meta, history.last_seq, history.page_count > 0, |cursor| {
            crate::storage::history::get_page(&meta.id, Some(&history.generation_id), cursor)
        })?;
    peer.event_offset = Some(history.source_size);
    Ok((origin, peer, messages))
}

fn build_seed_with(
    meta: &RunMeta,
    seq: u64,
    has_history: bool,
    mut read_page: impl FnMut(Option<&str>) -> Result<HistoryPage, String>,
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
    let mut cursor: Option<String> = None;
    if has_history {
        loop {
            let page = read_page(cursor.as_deref())?;
            // History pages arrive newest first, with entries in chronological
            // order. Keep only the latest 40 messages while counting all pages.
            for entry in page.entries.iter().rev() {
                let (assistant, text, ts, event_seq) = match entry {
                    HistoryEntry::User {
                        content,
                        ts,
                        last_seq,
                        ..
                    } => (false, &content.preview, ts, *last_seq),
                    HistoryEntry::Assistant {
                        content,
                        ts,
                        last_seq,
                        ..
                    } => (true, &content.preview, ts, *last_seq),
                    _ => continue,
                };
                if text.trim().is_empty() {
                    continue;
                }
                message_count += 1;
                if messages.len() < 40 {
                    messages.push_front(Message {
                        id: uuid::Uuid::new_v4().to_string(),
                        sender: if assistant { name } else { "Human" }.into(),
                        body: short_text(text, 8_000),
                        created_at: if ts.is_empty() { &meta.started_at } else { ts }.clone(),
                        participant_id: assistant.then(|| peer_id.clone()),
                        target_participant_id: None,
                        target_participant_ids: vec![],
                        source_event_id: Some(format!("{}:{event_seq}", meta.id)),
                        sidechat_id: None,
                        attachments: vec![],
                    });
                }
            }
            if !page.has_more {
                break;
            }
            let next = page
                .previous_cursor
                .ok_or("Conversation history has no previous page.")?;
            if cursor.as_ref() == Some(&next) {
                return Err("Conversation history did not advance.".into());
            }
            cursor = Some(next);
        }
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

    fn history_message(seq: u64) -> HistoryEntry {
        let content = crate::storage::history::HistoryContent {
            preview: format!("Historical message {seq}"),
            byte_length: 21,
            truncated: false,
            content_id: None,
            encoding: "utf8".into(),
        };
        if seq % 2 == 0 {
            HistoryEntry::User {
                id: seq.to_string(),
                anchor_id: seq.to_string(),
                ts: "2026-09-29T12:00:00Z".into(),
                content,
                cli_uuid: None,
                attachments: vec![],
                first_seq: seq,
                last_seq: seq,
            }
        } else {
            HistoryEntry::Assistant {
                id: seq.to_string(),
                anchor_id: seq.to_string(),
                ts: "2026-09-29T12:00:00Z".into(),
                content,
                thinking: None,
                model: None,
                first_seq: seq,
                last_seq: seq,
            }
        }
    }

    #[test]
    fn seed_retains_both_providers_identity_model_and_history_watermark() {
        for provider in ["codex", "claude"] {
            let source = meta(provider);
            let mut calls = 0;
            let (origin, peer, messages) = build_seed_with(&source, 90, true, |cursor| {
                calls += 1;
                assert_eq!(cursor, if calls == 1 { None } else { Some("older") });
                let (start, end) = if calls == 1 { (31, 45) } else { (1, 30) };
                Ok(HistoryPage {
                    run_id: source.id.clone(),
                    generation_id: "generation".into(),
                    entries: (start..=end).map(history_message).collect(),
                    page_cursor: if calls == 1 { "latest" } else { "older" }.into(),
                    previous_cursor: (calls == 1).then(|| "older".into()),
                    has_more: calls == 1,
                    first_seq: start,
                    last_seq: end,
                })
            })
            .unwrap();
            assert_eq!(origin.session_id, "saved-thread");
            assert_eq!(origin.message_count, 45);
            assert_eq!(peer.run_id, source.id);
            assert_eq!(peer.model.as_deref(), Some("recorded-model"));
            assert!(peer.paused);
            assert_eq!(peer.event_cursor, 90);
            assert_eq!(peer.message_cursor, 40);
            assert_eq!(messages[0].body, "Historical message 6");
            assert_eq!(messages[0].created_at, "2026-09-29T12:00:00Z");
            assert!(origin.context.contains("Historical message 45"));
            assert!(origin.context.contains("Continue Nemo"));
        }
    }

    #[test]
    fn empty_history_preserves_watermark_without_reading_a_page() {
        let source = meta("codex");
        let (origin, peer, messages) = build_seed_with(&source, 17, false, |_| {
            panic!("Empty histories have no page")
        })
        .unwrap();
        assert_eq!(origin.message_count, 0);
        assert_eq!(peer.event_cursor, 17);
        assert!(messages.is_empty());
    }

    #[test]
    fn projected_message_preview_is_bounded_and_reader_errors_propagate() {
        let source = meta("codex");
        let mut entry = history_message(2);
        if let HistoryEntry::User { content, .. } = &mut entry {
            content.preview = "你".repeat(12_000);
            content.byte_length = 2_000_000;
            content.truncated = true;
            content.content_id = Some("externalized-content".into());
        }
        let (_, _, messages) = build_seed_with(&source, 5916, true, |_| {
            Ok(HistoryPage {
                run_id: source.id.clone(),
                generation_id: "generation".into(),
                entries: vec![entry.clone()],
                page_cursor: "latest".into(),
                previous_cursor: None,
                has_more: false,
                first_seq: 2,
                last_seq: 2,
            })
        })
        .unwrap();
        assert!(messages[0].body.len() <= 8_003);
        assert!(messages[0].body.ends_with('…'));
        assert_eq!(messages[0].source_event_id.as_deref(), Some("origin-run:2"));
        assert!(
            build_seed_with(&source, 0, true, |_| Err("read failed".into()))
                .unwrap_err()
                .contains("read failed")
        );
    }

    #[test]
    #[ignore = "Local acceptance: requires an isolated copy of the failing RAV run"]
    fn live_large_rav_history_creates_paused_room() {
        let run_id = "92815e23-02b8-4436-8e0f-f7389ce80bfe";
        let source = crate::storage::runs::get_run(run_id).expect("Copied RAV run");
        let mut seq = 0;
        let mut offset = None;
        let error = loop {
            match crate::storage::events::list_bus_events_page(run_id, seq, offset) {
                Ok(page) => {
                    assert!(page.has_more, "Expected the reported oversized event");
                    seq = page.last_seq;
                    offset = Some(page.next_offset);
                }
                Err(error) => break error,
            }
        };
        assert!(error.contains("bus event 5915 exceeds catch-up page limit"));
        let started = std::time::Instant::now();
        let (origin, peer, messages) = build_seed(&source).unwrap();
        let watermark = peer.event_cursor;
        assert_eq!(watermark, crate::storage::events::next_seq(run_id) - 1);
        assert_eq!(messages.len(), 40);
        assert!(origin.message_count > 40);
        assert_eq!(origin.session_id, "01a080fc-5d5b-71a2-85ff-6913f24b98b4");
        let temp = tempfile::tempdir().unwrap();
        let store =
            crate::rooms::store::RoomStore::open(&temp.path().join("rooms.sqlite3")).unwrap();
        let room = store
            .create_from_session(
                super::super::models::CreateRoomInput {
                    title: source_title(&source),
                    objective: source.prompt.clone(),
                    repo_path: source.cwd.clone(),
                    repository: "ivg-design/rive-animation-viewer".into(),
                    create_project: false,
                },
                origin,
                peer,
                messages,
            )
            .unwrap();
        assert!(room.paused && room.participants[0].paused);
        crate::rooms::runtime::import_events(&store, &room.id, &room.participants[0]).unwrap();
        let reopened = store.get(&room.id).unwrap();
        assert_eq!(reopened.messages.len(), 40);
        assert_eq!(reopened.participants[0].event_cursor, watermark);
        assert!(reopened.runtime_error.is_none());
        eprintln!(
            "RAV room seed: {} messages, watermark {}, {:?}",
            reopened.origin.as_ref().unwrap().message_count,
            watermark,
            started.elapsed()
        );
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

use super::*;
use crate::rooms::{
    models::{CreateRoomInput, Participant},
    runtime,
};
use std::sync::Arc;
pub(super) struct Fixture {
    pub store: Arc<RoomStore>,
    pub principal: Principal,
    pub room: String,
    pub agent: String,
    pub dir: tempfile::TempDir,
}
pub(super) fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(RoomStore::open(&dir.path().join("rooms.sqlite3")).unwrap());
    let room = store
        .create(CreateRoomInput {
            title: "Isolated bridge".into(),
            objective: "Test without provider processes".into(),
            repo_path: dir.path().to_string_lossy().into(),
            repository: "fixture/project".into(),
            create_project: false,
        })
        .unwrap();
    let peer:Participant=serde_json::from_value(serde_json::json!({"id":"peer","name":"Fixture agent","provider":"codex","run_id":"fixture-run","paused":false})).unwrap();
    store
        .update(&room.id, |r| {
            r.paused = false;
            r.auto_continue = false;
            r.participants.push(peer);
            Ok(())
        })
        .unwrap();
    let principal = Principal {
        id: "fixture-principal".into(),
        token_sha256: String::new(),
        bindings: vec![Binding {
            room_id: room.id.clone(),
            conversation_ref: "fixture-conversation".into(),
        }],
        callback_hosts: vec!["callback.example.org".into()],
        scopes: vec!["read".into(), "send".into(), "subscribe".into()],
    };
    Fixture {
        agent: format!("{}/peer", room.id),
        room: room.id,
        store,
        principal,
        dir,
    }
}
fn message(f: &Fixture, id: &str, text: &str) -> Send {
    Send {
        agent_id: f.agent.clone(),
        text: text.into(),
        client_message_id: id.into(),
        conversation_ref: "fixture-conversation".into(),
        reply_to_message_id: None,
        mode: "queue".into(),
        attachments: vec![],
    }
}
fn reserve(f: &Fixture) -> String {
    let room = f.store.get(&f.room).unwrap();
    let plan = runtime::plan(&room, &room.participants[0], 1).unwrap();
    let room = runtime::reserve_delivery(&f.store, &f.room, "peer", plan, 1).unwrap();
    room.participants[0]
        .pending_delivery
        .as_ref()
        .unwrap()
        .text
        .clone()
}
fn start(f: &Fixture) {
    f.store
        .update(&f.room, |r| {
            let p = &mut r.participants[0];
            let d = p.pending_delivery.as_mut().unwrap();
            d.state = "sent".into();
            d.turn_started = true;
            d.provider_turn_id = Some("fixture-turn".into());
            p.state = "busy".into();
            Ok(())
        })
        .unwrap();
}
fn complete(f: &Fixture) {
    f.store
        .update_projected(&f.room, |r, c| {
            let before = snapshot(c, r)?.unwrap();
            r.participants[0].pending_delivery = None;
            r.participants[0].state = "idle".into();
            reconcile(c, &before, r, true)
        })
        .unwrap();
}
fn visible(f: &Fixture, text: &str) {
    f.store
        .update(&f.room, |r| {
            r.messages.push(Message {
                id: uuid::Uuid::new_v4().to_string(),
                sender: "Fixture agent".into(),
                body: text.into(),
                created_at: crate::models::now_iso(),
                participant_id: Some("peer".into()),
                target_participant_id: None,
                target_participant_ids: vec![],
                source_event_id: Some("fixture-visible".into()),
                sidechat_id: None,
                attachments: vec![],
            });
            Ok(())
        })
        .unwrap();
}
#[test]
fn idle_owned_scheduler_receipt_and_visible_replay_survive_restart() {
    let f = fixture();
    let receipt = f
        .store
        .bridge_send(&f.principal, &message(&f, "one", "fixture request"))
        .unwrap();
    assert_eq!(receipt.state, "queued");
    reserve(&f);
    assert_eq!(
        f.store
            .bridge_get(&f.principal, &receipt.message_id)
            .unwrap()
            .state,
        "queued"
    );
    start(&f);
    let receipt = f
        .store
        .bridge_get(&f.principal, &receipt.message_id)
        .unwrap();
    assert_eq!(receipt.state, "running");
    assert_eq!(receipt.provider_turn_id.as_deref(), Some("fixture-turn"));
    assert!(receipt.delivered_at.is_some());
    visible(&f, "Visible answer");
    complete(&f);
    assert_eq!(
        f.store
            .bridge_get(&f.principal, &receipt.message_id)
            .unwrap()
            .state,
        "completed"
    );
    let reopened = RoomStore::open(&f.dir.path().join("rooms.sqlite3")).unwrap();
    let replies = reopened
        .bridge_replies(&f.principal, "fixture-conversation", 0, 10)
        .unwrap();
    assert_eq!(replies["replies"].as_array().unwrap().len(), 1);
    assert_eq!(replies["replies"][0]["text"], "Visible answer");
    assert_eq!(
        replies["replies"][0]["reply_to_message_id"],
        receipt.message_id
    );
    let cursor = replies["next_cursor"].as_i64().unwrap();
    assert!(reopened
        .bridge_replies(&f.principal, "fixture-conversation", cursor, 10)
        .unwrap()["replies"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        reopened
            .bridge_send(&f.principal, &message(&f, "one", "fixture request"))
            .unwrap()
            .message_id,
        receipt.message_id
    );
}
#[test]
fn duplicates_conflicts_and_concurrent_retries_are_atomic() {
    let f = fixture();
    let input = message(&f, "same", "same payload");
    std::thread::scope(|s| {
        let calls = (0..8)
            .map(|_| {
                s.spawn(|| {
                    f.store
                        .bridge_send(&f.principal, &input)
                        .unwrap()
                        .message_id
                })
            })
            .collect::<Vec<_>>();
        let ids = calls
            .into_iter()
            .map(|c| c.join().unwrap())
            .collect::<Vec<_>>();
        assert!(ids.iter().all(|id| id == &ids[0]));
    });
    assert_eq!(f.store.get(&f.room).unwrap().messages.len(), 1);
    assert!(f
        .store
        .bridge_send(&f.principal, &message(&f, "same", "different"))
        .unwrap_err()
        .contains("conflict"));
    let receipt = f.store.bridge_send(&f.principal, &input).unwrap();
    let mut other = f.principal.clone();
    other.id = "other".into();
    assert!(f.store.bridge_get(&other, &receipt.message_id).is_err());
    let mut wrong = input.clone();
    wrong.conversation_ref = "arbitrary-dot".into();
    assert!(f.store.bridge_send(&f.principal, &wrong).is_err());
}
#[test]
fn two_external_messages_are_separate_turns_and_future_payload_does_not_leak() {
    let f = fixture();
    let a = f
        .store
        .bridge_send(&f.principal, &message(&f, "a", "FIRST_UNIQUE_PAYLOAD"))
        .unwrap();
    let b = f
        .store
        .bridge_send(&f.principal, &message(&f, "b", "SECOND_UNIQUE_PAYLOAD"))
        .unwrap();
    let prompt = reserve(&f);
    assert!(prompt.contains("FIRST_UNIQUE_PAYLOAD"));
    assert!(!prompt.contains("SECOND_UNIQUE_PAYLOAD"));
    start(&f);
    visible(&f, "first answer");
    complete(&f);
    assert_eq!(
        f.store
            .bridge_get(&f.principal, &a.message_id)
            .unwrap()
            .state,
        "completed"
    );
    assert_eq!(
        f.store
            .bridge_get(&f.principal, &b.message_id)
            .unwrap()
            .state,
        "queued"
    );
    assert!(reserve(&f).contains("SECOND_UNIQUE_PAYLOAD"));
    start(&f);
    visible(&f, "second answer");
    complete(&f);
    let replies = f
        .store
        .bridge_replies(&f.principal, "fixture-conversation", 0, 10)
        .unwrap();
    assert_eq!(replies["replies"][0]["reply_to_message_id"], a.message_id);
    assert_eq!(replies["replies"][1]["reply_to_message_id"], b.message_id);
}
#[test]
fn busy_queue_preserves_current_turn_and_permissions_pauses_and_budget() {
    let f = fixture();
    f.store
        .bridge_send(&f.principal, &message(&f, "active", "active"))
        .unwrap();
    reserve(&f);
    start(&f);
    let queued = f
        .store
        .bridge_send(&f.principal, &message(&f, "queued", "queued while busy"))
        .unwrap();
    let room = f.store.get(&f.room).unwrap();
    assert!(runtime::plan(&room, &room.participants[0], 2).is_none());
    assert_eq!(
        f.store
            .bridge_get(&f.principal, &queued.message_id)
            .unwrap()
            .state,
        "queued"
    );
    complete(&f);
    for gate in ["room", "peer", "budget", "permission"] {
        f.store
            .update(&f.room, |r| {
                r.paused = gate == "room";
                let p = &mut r.participants[0];
                p.paused = gate == "peer";
                p.state = if gate == "permission" {
                    "waiting"
                } else {
                    "idle"
                }
                .into();
                p.max_turns = if gate == "budget" { 1 } else { 0 };
                p.wake_count = 1;
                Ok(())
            })
            .unwrap();
        let r = f.store.get(&f.room).unwrap();
        assert!(runtime::plan(&r, &r.participants[0], 2).is_none(), "{gate}");
        let before = serde_json::to_string(&r.participants[0]).unwrap();
        f.store
            .bridge_send(&f.principal, &message(&f, gate, "no implicit wake"))
            .unwrap();
        let after = f.store.get(&f.room).unwrap();
        assert_eq!(
            serde_json::to_string(&after.participants[0]).unwrap(),
            before
        );
        assert_eq!(after.paused, r.paused);
    }
    let mut steer = message(&f, "steer", "hello");
    steer.mode = "steer".into();
    assert!(f.store.bridge_send(&f.principal, &steer).is_err());
}
#[test]
fn lost_delivery_is_ambiguous_and_removed_agent_is_cancelled() {
    let f = fixture();
    let receipt = f
        .store
        .bridge_send(&f.principal, &message(&f, "ambiguous", "unknown"))
        .unwrap();
    reserve(&f);
    start(&f);
    f.store
        .update(&f.room, |r| {
            r.participants[0].pending_delivery = None;
            r.participants[0].state = "idle".into();
            Ok(())
        })
        .unwrap();
    assert_eq!(
        f.store
            .bridge_get(&f.principal, &receipt.message_id)
            .unwrap()
            .state,
        "ambiguous"
    );
    let queued = f
        .store
        .bridge_send(&f.principal, &message(&f, "cancelled", "removed"))
        .unwrap();
    f.store
        .update(&f.room, |r| {
            r.participants.clear();
            Ok(())
        })
        .unwrap();
    assert_eq!(
        f.store
            .bridge_get(&f.principal, &queued.message_id)
            .unwrap()
            .state,
        "cancelled"
    );

    let g = fixture();
    let interrupted = g
        .store
        .bridge_send(&g.principal, &message(&g, "stopped", "may have run"))
        .unwrap();
    reserve(&g);
    start(&g);
    g.store
        .update(&g.room, |r| {
            r.participants[0].pending_delivery = None;
            r.participants[0].paused = true;
            r.participants[0].state = "paused".into();
            Ok(())
        })
        .unwrap();
    assert_eq!(
        g.store
            .bridge_get(&g.principal, &interrupted.message_id)
            .unwrap()
            .state,
        "ambiguous"
    );
}

#[test]
fn paused_waiting_delivery_keeps_awaiting_input_receipt_live() {
    let f = fixture();
    let receipt = f
        .store
        .bridge_send(&f.principal, &message(&f, "approval", "needs approval"))
        .unwrap();
    reserve(&f);
    f.store
        .update(&f.room, |r| {
            let peer = &mut r.participants[0];
            peer.paused = true;
            peer.state = "waiting".into();
            peer.last_error = Some("Waiting for approval".into());
            Ok(())
        })
        .unwrap();
    assert_eq!(
        f.store
            .bridge_get(&f.principal, &receipt.message_id)
            .unwrap()
            .state,
        "awaiting_input"
    );

    complete(&f);
    assert_eq!(
        f.store
            .bridge_get(&f.principal, &receipt.message_id)
            .unwrap()
            .state,
        "completed"
    );
}
#[test]
fn attachments_are_existing_room_ids_and_unknown_schema_is_rejected() {
    let f = fixture();
    let mut input = message(&f, "attachments", "unsafe attachment");
    input.attachments.push("../../other-room".into());
    assert!(f.store.bridge_send(&f.principal, &input).is_err());
    let raw = serde_json::json!({"agent_id":f.agent,"text":"hello","client_message_id":"spoof","conversation_ref":"fixture-conversation","principal":"admin"});
    assert!(serde_json::from_value::<Send>(raw).is_err());
}
#[tokio::test]
async fn protocol_auth_scope_and_exact_tool_names() {
    let f = fixture();
    let call = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ocv.send_message","arguments":message(&f,"protocol","visible")}});
    assert_eq!(
        protocol::dispatch(&f.store, &f.principal, &call).await["result"]["structuredContent"]
            ["state"],
        "queued"
    );
    let mut blocked = f.principal.clone();
    blocked.scopes = vec!["read".into()];
    assert!(protocol::dispatch(&f.store, &blocked, &call)
        .await
        .get("error")
        .is_some());
    let unknown = serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"send_message_to_thread","arguments":{}}});
    assert!(protocol::dispatch(&f.store, &f.principal, &unknown)
        .await
        .get("error")
        .is_some());
}

#[test]
fn owner_access_is_private_bound_and_revocable_without_credentials_in_the_profile() {
    let f = fixture();
    let path = f.dir.path().join("fixture-access.json");
    let token = "fixture-only-token-not-a-real-credential-123456789";
    let hash = format!("{:x}", Sha256::digest(token.as_bytes()));
    std::fs::write(&path,serde_json::json!({"principals":[{"id":f.principal.id,"token_sha256":hash,"bindings":[{"room_id":f.room,"conversation_ref":"fixture-conversation"}],"callback_hosts":[],"scopes":["read"]}]}).to_string()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    assert!(authenticate_access(read_access(&path).unwrap(), token).is_ok());
    assert!(authenticate_access(
        read_access(&path).unwrap(),
        "incorrect-but-long-enough-token-12345678"
    )
    .is_err());
    let mut grants = f.principal.clone();
    grants.bindings.push(Binding {
        room_id: "second-room".into(),
        conversation_ref: "second-conversation".into(),
    });
    let mut cross = message(&f, "cross", "wrong correlation");
    cross.conversation_ref = "second-conversation".into();
    assert!(f
        .store
        .bridge_send(&grants, &cross)
        .unwrap_err()
        .contains("binding"));
    std::fs::write(&path, "{\"principals\":[]}").unwrap();
    assert!(authenticate_access(read_access(&path).unwrap(), token).is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(read_access(&path).is_err());
    }
}
#[test]
fn idempotent_attachment_receipt_survives_removed_file() {
    let f = fixture();
    let file = f.dir.path().join("fixture.txt");
    std::fs::write(&file, "fixture").unwrap();
    let attachment = f
        .store
        .attach_files(&f.room, &[file.to_string_lossy().into_owned()])
        .unwrap()
        .remove(0);
    let mut input = message(&f, "file", "with attachment");
    input.attachments.push(attachment.id.clone());
    let receipt = f.store.bridge_send(&f.principal, &input).unwrap();
    std::fs::remove_dir_all(f.store.attachment_root.join(&f.room).join(attachment.id)).unwrap();
    assert_eq!(
        f.store
            .bridge_send(&f.principal, &input)
            .unwrap()
            .message_id,
        receipt.message_id
    );
}

#[tokio::test]
async fn claude_mcp_post_and_streamed_reply_project_once_with_durable_correlation() {
    let f = fixture();
    f.store
        .update(&f.room, |r| {
            r.participants[0].provider = "claude".into();
            Ok(())
        })
        .unwrap();
    for turn in 1..=2 {
        let receipt = f
            .store
            .bridge_send(
                &f.principal,
                &message(&f, &format!("claude-turn-{turn}"), "isolated task"),
            )
            .unwrap();
        reserve(&f);
        start(&f);
        let posted = crate::rooms::mcp::call_tool(
            f.dir.path(),
            &f.room,
            "peer",
            "post_message",
            &serde_json::json!({"body":"Repeated visible answer"}),
        )
        .await
        .unwrap();
        assert_eq!(posted["message_posted"], true);
        let mut protocol = crate::agent::claude_protocol::ProtocolState::new(false);
        let events=protocol.map_event("fixture-run",&serde_json::json!({"type":"assistant","message":{"id":format!("fixture-assistant-{turn}"),"model":"fixture-claude","stop_reason":"end_turn","content":[{"type":"thinking","thinking":"HIDDEN_FIXTURE_REASONING"},{"type":"text","text":"Repeated visible answer"}]}}));
        let base = (turn - 1) * 10;
        let mut lines=events.iter().enumerate().map(|(index,event)|format!("{}\n",serde_json::json!({"_bus":true,"seq":base+index+1,"ts":crate::models::now_iso(),"event":event}))).collect::<String>();
        lines.push_str(&format!("{}\n",serde_json::json!({"_bus":true,"seq":base+9,"ts":crate::models::now_iso(),"event":{"type":"run_state","state":"completed"}})));
        let page = crate::storage::room_events::page_from_reader(
            &mut std::io::Cursor::new(lines.as_bytes()),
            base as u64,
        )
        .unwrap();
        let peer = f.store.get(&f.room).unwrap().participants.remove(0);
        runtime::import_page(&f.store, &f.room, &peer, &page).unwrap();
        runtime::import_page(&f.store, &f.room, &peer, &page).unwrap();
        let room = f.store.get(&f.room).unwrap();
        assert_eq!(
            room.messages
                .iter()
                .filter(|m| m.body == "Repeated visible answer")
                .count(),
            turn
        );
        assert_eq!(
            f.store
                .bridge_get(&f.principal, &receipt.message_id)
                .unwrap()
                .state,
            "completed"
        );
        let replies = f
            .store
            .bridge_replies(&f.principal, "fixture-conversation", 0, 10)
            .unwrap();
        assert_eq!(replies["replies"].as_array().unwrap().len(), turn);
        assert!(!replies.to_string().contains("HIDDEN_FIXTURE_REASONING"));
    }
}

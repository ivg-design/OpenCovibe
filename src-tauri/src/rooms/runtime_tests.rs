use super::{plan, queue_due_timers, recover, reserve_delivery};
use crate::rooms::{
    models::{BoardItem, Claim, CreateRoomInput, Delivery, Participant, Room, Timer},
    store::RoomStore,
};
use std::{
    path::Path,
    sync::{Arc, Barrier},
};
use tempfile::TempDir;

struct Fixture {
    _temp: TempDir,
    store: RoomStore,
    room: Room,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let store = RoomStore::open(&temp.path().join("rooms.sqlite3")).unwrap();
        let created = store
            .create(CreateRoomInput {
                title: "Runtime test room".into(),
                objective: "Exercise local runtime decisions".into(),
                repo_path: temp.path().to_string_lossy().into_owned(),
                repository: "owner/repository".into(),
                create_project: false,
            })
            .unwrap();
        let room = store
            .update(&created.id, |room| {
                room.paused = false;
                room.board.synced_at = Some(crate::models::now_iso());
                room.board.items = ["task-a", "task-b", "task-c", "task-d"]
                    .iter()
                    .map(|id| item(id))
                    .collect();
                room.participants = vec![
                    participant("peer-a", "Codex"),
                    participant("peer-b", "Claude"),
                ];
                Ok(())
            })
            .unwrap();
        Self {
            _temp: temp,
            store,
            room,
        }
    }

    fn peer(&self, id: &str) -> Participant {
        self.room
            .participants
            .iter()
            .find(|peer| peer.id == id)
            .unwrap()
            .clone()
    }

    fn now(&self) -> i64 {
        chrono::Utc::now().timestamp_millis() + 100
    }
}

fn import_message_complete(fixture: &Fixture, seq: u64, message_id: &str, text: &str) {
    let room = fixture.store.get(&fixture.room.id).unwrap();
    let peer = room
        .participants
        .iter()
        .find(|peer| peer.id == "peer-a")
        .unwrap();
    let page = crate::storage::events::BusEventPage {
        events: vec![serde_json::json!({
            "_seq": seq,
            "type": "message_complete",
            "message_id": message_id,
            "text": text,
        })],
        last_seq: seq,
        has_more: false,
        next_offset: seq,
    };
    super::import_page(&fixture.store, &room.id, peer, &page).unwrap();
}

fn item(id: &str) -> BoardItem {
    BoardItem {
        id: id.into(),
        title: format!("Task {id}"),
        url: None,
        status: "Ready".into(),
        priority: None,
        agent: None,
        kind: "issue".into(),
        body: None,
        number: None,
        assignees: vec![],
        labels: vec![],
        linked_prs: vec![],
        updated_at: None,
    }
}

fn participant(id: &str, name: &str) -> Participant {
    Participant {
        id: id.into(),
        name: name.into(),
        provider: "codex".into(),
        run_id: format!("run-{id}"),
        paused: false,
        state: "idle".into(),
        max_turns: 20,
        ..Default::default()
    }
}

#[test]
fn older_room_records_get_runtime_field_defaults() {
    let fixture = Fixture::new();
    let mut value = serde_json::to_value(&fixture.room).unwrap();
    value.as_object_mut().unwrap().remove("max_concurrent");
    value.as_object_mut().unwrap().remove("sidechats");
    for peer in value["participants"].as_array_mut().unwrap() {
        peer.as_object_mut().unwrap().remove("no_progress_turns");
        peer.as_object_mut().unwrap().remove("work_signature");
        peer.as_object_mut().unwrap().remove("active_sidechat_id");
        peer.as_object_mut().unwrap().remove("read_message_ids");
        peer.as_object_mut().unwrap().remove("unread_message_ids");
    }
    value["timers"] = serde_json::json!([{
        "id":"timer-old", "participant_id":"peer-a", "message":"hello",
        "interval_seconds":30, "idle_only":false, "enabled":true,
        "next_due_at":1, "max_deliveries":2, "delivered_count":0, "last_error":null
    }]);

    let restored: Room = serde_json::from_value(value).unwrap();
    assert_eq!(restored.max_concurrent, 3);
    assert_eq!(restored.participants[0].no_progress_turns, 0);
    assert_eq!(restored.participants[0].work_signature, None);
    assert_eq!(restored.timers[0].queued_at, None);
    assert_eq!(restored.timers[0].max_deliveries, Some(2));
    assert_eq!(restored.timers[0].ends_at, None);
    assert!(restored.sidechats.is_empty());
    assert_eq!(restored.participants[0].active_sidechat_id, None);
}

#[test]
fn sidechat_delivery_acknowledges_only_its_branch_and_retains_other_unread_messages() {
    let fixture = Fixture::new();
    let source = fixture
        .store
        .append_message(
            &fixture.room.id,
            "Human",
            "Start a focused discussion".into(),
            None,
            None,
            None,
        )
        .unwrap()
        .messages
        .last()
        .unwrap()
        .id
        .clone();
    let room = fixture
        .store
        .create_sidechat(
            &fixture.room.id,
            &source,
            "Focused discussion",
            vec!["peer-a".into(), "peer-b".into()],
        )
        .unwrap();
    let sidechat_id = room.sidechats[0].id.clone();
    fixture
        .store
        .append_message_in_sidechat(
            &room.id,
            "Human",
            "Sidechat question".into(),
            None,
            None,
            None,
            Some(sidechat_id.clone()),
        )
        .unwrap();
    fixture
        .store
        .append_message(
            &room.id,
            "Human",
            "Separate main question".into(),
            None,
            None,
            None,
        )
        .unwrap();
    let current = fixture.store.get(&room.id).unwrap();
    let peer = current
        .participants
        .iter()
        .find(|p| p.id == "peer-a")
        .unwrap()
        .clone();
    let delivery = plan(&current, &peer, fixture.now()).unwrap();
    assert_eq!(delivery.sidechat_id.as_deref(), Some(sidechat_id.as_str()));
    reserve_delivery(&fixture.store, &room.id, &peer.id, delivery, fixture.now()).unwrap();
    let reserved = fixture.store.get(&room.id).unwrap();
    let reserved_peer = reserved
        .participants
        .iter()
        .find(|p| p.id == "peer-a")
        .unwrap();
    assert!(reserved_peer.active_sidechat_id.as_deref() == Some(sidechat_id.as_str()));
    assert!(reserved_peer.read_message_ids.iter().any(|id| reserved
        .messages
        .iter()
        .any(|m| &m.id == id && m.sidechat_id.as_deref() == Some(sidechat_id.as_str()))));
    assert!(!reserved_peer.read_message_ids.iter().any(|id| reserved
        .messages
        .iter()
        .any(|m| &m.id == id && m.body == "Separate main question")));
    let mut idle_peer = reserved_peer.clone();
    idle_peer.state = "idle".into();
    idle_peer.pending_delivery = None;
    let next = plan(&reserved, &idle_peer, fixture.now()).unwrap();
    assert!(next.sidechat_id.is_none());
}

fn claim(task_id: &str, participant_id: &str, state: &str) -> Claim {
    Claim {
        task_id: task_id.into(),
        participant_id: participant_id.into(),
        state: state.into(),
        updated_at: crate::models::now_iso(),
        summary: None,
        evidence: None,
    }
}

fn delivery(reason: &str, timer_id: Option<&str>, created_at: i64) -> Delivery {
    Delivery {
        id: format!("delivery-{created_at}"),
        reason: reason.into(),
        text: "test wake".into(),
        created_at,
        state: "prepared".into(),
        turn_started: true,
        provider_turn_id: None,
        task_id: None,
        timer_id: timer_id.map(str::to_owned),
        sidechat_id: None,
        message_id: None,
        attachment_ids: vec![],
    }
}

fn timer(id: &str, participant_id: &str, now: i64) -> Timer {
    Timer {
        id: id.into(),
        participant_id: participant_id.into(),
        message: "Periodic check".into(),
        interval_seconds: 30,
        idle_only: true,
        enabled: true,
        next_due_at: now - 1,
        queued_at: None,
        max_deliveries: Some(2),
        ends_at: None,
        delivered_count: 0,
        last_error: None,
    }
}

#[test]
fn idle_peer_selects_one_eligible_task() {
    let fixture = Fixture::new();
    let peer = fixture.peer("peer-a");
    let decision = plan(&fixture.room, &peer, fixture.now()).unwrap();
    assert_eq!(decision.reason, "task");
    assert_eq!(decision.task_id.as_deref(), Some("task-a"));
}

#[test]
fn existing_active_self_claim_is_prioritized_over_new_board_work() {
    let mut fixture = Fixture::new();
    fixture
        .room
        .claims
        .push(claim("task-c", "peer-a", "active"));
    let peer = fixture.peer("peer-a");
    let decision = plan(&fixture.room, &peer, fixture.now()).unwrap();
    assert_eq!(decision.task_id.as_deref(), Some("task-c"));
}

#[test]
fn other_owned_uncertain_and_blocked_tasks_are_skipped() {
    let mut fixture = Fixture::new();
    fixture.room.claims = vec![
        claim("task-a", "peer-b", "active"),
        claim("task-b", "peer-b", "uncertain"),
        claim("task-c", "peer-b", "blocked"),
    ];
    let peer = fixture.peer("peer-a");
    let decision = plan(&fixture.room, &peer, fixture.now()).unwrap();
    assert_eq!(decision.task_id.as_deref(), Some("task-d"));
}

#[test]
fn human_everyone_revives_dormant_peers_and_delivers_without_releasing_blocked_claims() {
    let fixture = Fixture::new();
    fixture
        .store
        .update(&fixture.room.id, |room| {
            room.auto_continue = false;
            room.claims.push(claim("task-a", "peer-a", "blocked"));
            for (peer, state) in room.participants.iter_mut().zip(["blocked", "no_progress"]) {
                peer.paused = true;
                peer.state = state.into();
                peer.no_progress_turns = 3;
                peer.last_error = Some("Earlier task needs input".into());
            }
            Ok(())
        })
        .unwrap();
    let room = fixture
        .store
        .post_message(
            &fixture.room.id,
            "@everyone please review the new requirements".into(),
        )
        .unwrap();
    assert_eq!(room.claims[0].state, "blocked");
    for peer in &room.participants {
        assert!(!peer.paused);
        assert_eq!(peer.state, "idle");
        assert_eq!(peer.no_progress_turns, 0);
        let next = plan(&room, peer, fixture.now()).unwrap();
        assert_eq!(next.reason, "message");
        let reserved =
            reserve_delivery(&fixture.store, &room.id, &peer.id, next, fixture.now()).unwrap();
        let delivery = reserved
            .participants
            .iter()
            .find(|p| p.id == peer.id)
            .unwrap()
            .pending_delivery
            .as_ref()
            .unwrap();
        assert!(delivery
            .text
            .contains("@everyone please review the new requirements"));
        assert!(delivery
            .text
            .contains("Treat the room GitHub Project as the live work record"));
        assert!(delivery.text.contains("room.update_task"));
    }
    let reopened = RoomStore::open(&fixture._temp.path().join("rooms.sqlite3"))
        .unwrap()
        .get(&room.id)
        .unwrap();
    assert!(reopened
        .participants
        .iter()
        .all(|p| p.pending_delivery.is_some()));
}

#[test]
fn everyone_received_during_a_finishing_blocked_turn_wakes_after_completion() {
    let fixture = Fixture::new();
    fixture
        .store
        .update(&fixture.room.id, |room| {
            room.auto_continue = false;
            let peer = &mut room.participants[0];
            peer.paused = true;
            peer.state = "blocked".into();
            peer.pending_delivery = Some(delivery("task", None, fixture.now()));
            Ok(())
        })
        .unwrap();
    let room = fixture
        .store
        .append_message(
            &fixture.room.id,
            "Human",
            "@Codex new input after your blocker".into(),
            None,
            None,
            None,
        )
        .unwrap();
    assert!(room.participants[0].paused);
    let completed = fixture
        .store
        .update(&room.id, |room| {
            room.participants[0].pending_delivery = None;
            super::wake_dormant_for_unread_messages(room);
            Ok(())
        })
        .unwrap();
    assert!(!completed.participants[0].paused);
    assert_eq!(
        plan(&completed, &completed.participants[0], fixture.now())
            .unwrap()
            .reason,
        "message"
    );
}

#[test]
fn human_messages_revive_only_addressed_dormant_peers() {
    for (body, target) in [
        ("@Codex new instructions", None),
        ("New instructions", Some("peer-a")),
        ("@Codex @Claude new instructions", None),
        ("New instructions for everyone", None),
    ] {
        let fixture = Fixture::new();
        fixture
            .store
            .update(&fixture.room.id, |room| {
                for peer in &mut room.participants {
                    peer.paused = true;
                    peer.state = "blocked".into();
                }
                Ok(())
            })
            .unwrap();
        let room = fixture
            .store
            .append_message(
                &fixture.room.id,
                "Human",
                body.into(),
                None,
                target.map(str::to_owned),
                None,
            )
            .unwrap();
        assert!(!room.participants[0].paused);
        let everyone = body.contains("@Claude") || (target.is_none() && !body.contains('@'));
        assert_eq!(room.participants[1].paused, !everyone);
    }
}

#[test]
fn everyone_respects_manual_pauses_permission_waits_limits_and_inflight_deliveries() {
    for (state, pending, exhausted) in [
        ("paused", false, false),
        ("waiting", false, false),
        ("quota", false, false),
        ("failed", false, false),
        ("budget_exhausted", false, false),
        ("busy", true, false),
        ("blocked", true, false),
        ("no_progress", false, true),
    ] {
        let fixture = Fixture::new();
        fixture
            .store
            .update(&fixture.room.id, |room| {
                let peer = &mut room.participants[0];
                peer.state = state.into();
                peer.paused = true;
                if pending {
                    peer.pending_delivery = Some(delivery("message", None, fixture.now()));
                }
                if exhausted {
                    peer.wake_count = peer.max_turns;
                }
                Ok(())
            })
            .unwrap();
        let room = fixture
            .store
            .post_message(&fixture.room.id, "@everyone respond".into())
            .unwrap();
        assert!(room.participants[0].paused, "state {state}");
        assert_eq!(room.participants[0].state, state);
        assert!(plan(&room, &room.participants[0], fixture.now()).is_none());
    }
}

#[test]
fn historical_and_untargeted_agent_messages_do_not_revive_dormant_peers() {
    for (sender, peer, source) in [
        ("Human", None, Some("seed:history")),
        ("Claude", Some("peer-b"), Some("run-claude:1")),
        ("Not a room peer", Some("unknown"), None),
    ] {
        let fixture = Fixture::new();
        fixture
            .store
            .update(&fixture.room.id, |room| {
                room.participants[0].paused = true;
                room.participants[0].state = "blocked".into();
                Ok(())
            })
            .unwrap();
        let room = fixture
            .store
            .append_message(
                &fixture.room.id,
                sender,
                "@everyone respond".into(),
                peer.map(str::to_owned),
                None,
                source.map(str::to_owned),
            )
            .unwrap();
        assert!(room.participants[0].paused);
    }
}

#[test]
fn cold_actor_idle_preserves_reserved_turn_until_running_then_completion() {
    for provider in ["codex", "claude"] {
        let fixture = Fixture::new();
        fixture
            .store
            .update(&fixture.room.id, |r| {
                r.auto_continue = false;
                r.participants[0].provider = provider.into();
                r.participants[0].max_turns = 1;
                Ok(())
            })
            .unwrap();
        let room = fixture
            .store
            .append_message(
                &fixture.room.id,
                "Claude",
                "Review now".into(),
                Some("peer-b".into()),
                Some("peer-a".into()),
                None,
            )
            .unwrap();
        let next = plan(&room, &room.participants[0], fixture.now()).unwrap();
        let reserved =
            reserve_delivery(&fixture.store, &room.id, "peer-a", next, fixture.now()).unwrap();
        assert!(
            !reserved.participants[0]
                .pending_delivery
                .as_ref()
                .unwrap()
                .turn_started
        );
        let running = fixture
            .store
            .update(&room.id, |r| {
                super::mark_delivery_accepted(
                    r,
                    "peer-a",
                    &reserved.participants[0]
                        .pending_delivery
                        .as_ref()
                        .unwrap()
                        .id,
                    fixture.now(),
                )?;
                let peer = &mut r.participants[0];
                super::apply_event_state(
                    peer,
                    "run_state",
                    &serde_json::json!({"state":"idle"}),
                    None,
                );
                assert!(peer.pending_delivery.is_some());
                assert_eq!(peer.state, "busy");
                assert_eq!(peer.wake_count, 1);
                super::apply_event_state(
                    peer,
                    "run_state",
                    &serde_json::json!({"state":"running"}),
                    None,
                );
                assert!(peer.pending_delivery.as_ref().unwrap().turn_started);
                Ok(())
            })
            .unwrap();
        assert!(running.participants[0].pending_delivery.is_some());
        let reopened = RoomStore::open(&fixture._temp.path().join("rooms.sqlite3")).unwrap();
        let completed = reopened
            .update(&room.id, |r| {
                super::apply_event_state(
                    &mut r.participants[0],
                    "run_state",
                    &serde_json::json!({"state":"idle"}),
                    None,
                );
                Ok(())
            })
            .unwrap();
        assert!(completed.participants[0].pending_delivery.is_none());
        assert_eq!(completed.participants[0].wake_count, 1);
        assert!(plan(&completed, &completed.participants[0], fixture.now()).is_none());
    }
}

#[test]
fn legacy_in_flight_delivery_can_complete_after_upgrade() {
    let mut peer = participant("peer-a", "Codex");
    let mut value = serde_json::to_value(delivery("message", None, 1)).unwrap();
    value.as_object_mut().unwrap().remove("turn_started");
    peer.pending_delivery = Some(serde_json::from_value(value).unwrap());
    peer.state = "busy".into();
    super::apply_event_state(
        &mut peer,
        "run_state",
        &serde_json::json!({"state":"idle"}),
        None,
    );
    assert!(peer.pending_delivery.is_none());
    assert_eq!(peer.state, "idle");
}

#[test]
fn cold_actor_idle_error_still_fails_the_delivery() {
    let mut peer = participant("peer-a", "Codex");
    let mut intent = delivery("message", None, 1);
    intent.turn_started = false;
    peer.pending_delivery = Some(intent);
    peer.state = "busy".into();
    super::apply_event_state(
        &mut peer,
        "run_state",
        &serde_json::json!({"state":"idle","error":"startup failed"}),
        None,
    );
    assert!(peer.paused);
    assert_eq!(peer.state, "failed");
    assert!(peer.pending_delivery.is_none());
    assert_eq!(peer.last_error.as_deref(), Some("startup failed"));
}

#[test]
fn peer_addresses_wake_dormant_recipients_and_reserve_once() {
    for state in ["blocked", "no_progress", "completed"] {
        for (body, target, all) in [
            ("Please review the artifact", Some("peer-a"), false),
            ("@Codex please review the artifact", None, false),
            ("@Codex @Claude please review", None, true),
            ("@everyone please review", None, true),
        ] {
            let fixture = Fixture::new();
            fixture
                .store
                .update(&fixture.room.id, |room| {
                    room.auto_continue = false;
                    room.participants.push(participant("lead", "Lead"));
                    room.claims.push(claim("task-a", "peer-a", "blocked"));
                    for peer in &mut room.participants[..2] {
                        peer.paused = true;
                        peer.state = state.into();
                        peer.no_progress_turns = 3;
                    }
                    Ok(())
                })
                .unwrap();
            let room = fixture
                .store
                .append_message(
                    &fixture.room.id,
                    "Lead",
                    body.into(),
                    Some("lead".into()),
                    target.map(str::to_owned),
                    None,
                )
                .unwrap();
            assert!(!room.participants[0].paused, "{state}: {body}");
            assert_eq!(room.participants[1].paused, !all);
            assert_eq!(room.claims[0].state, "blocked");
            assert!(
                plan(&room, &room.participants[2], fixture.now()).is_none(),
                "sender must not reply to itself"
            );
            let next = plan(&room, &room.participants[0], fixture.now()).unwrap();
            let reserved =
                reserve_delivery(&fixture.store, &room.id, "peer-a", next, fixture.now()).unwrap();
            assert!(reserved.participants[0]
                .pending_delivery
                .as_ref()
                .unwrap()
                .text
                .contains(body));
            fixture
                .store
                .update(&room.id, |r| {
                    r.participants[0].pending_delivery = None;
                    r.participants[0].paused = true;
                    r.participants[0].state = state.into();
                    Ok(())
                })
                .unwrap();
            let reopened = RoomStore::open(&fixture._temp.path().join("rooms.sqlite3")).unwrap();
            let consumed = reopened
                .update(&room.id, |r| {
                    super::wake_dormant_for_unread_messages(r);
                    Ok(())
                })
                .unwrap();
            assert!(
                consumed.participants[0].paused,
                "already delivered input cannot revive again"
            );
        }
    }
}

#[test]
fn peer_broadcasts_code_mentions_and_historical_directed_output_do_not_revive() {
    for (body, target, source) in [
        ("Progress update for the room", None, None),
        ("Example: `@everyone`", None, None),
        ("Email hello@Codex", None, None),
        (
            "@everyone historical result",
            Some("peer-a"),
            Some("run-lead:2"),
        ),
    ] {
        let fixture = Fixture::new();
        fixture
            .store
            .update(&fixture.room.id, |r| {
                r.participants[0].paused = true;
                r.participants[0].state = "no_progress".into();
                Ok(())
            })
            .unwrap();
        let room = fixture
            .store
            .append_message(
                &fixture.room.id,
                "Claude",
                body.into(),
                Some("peer-b".into()),
                target.map(str::to_owned),
                source.map(str::to_owned),
            )
            .unwrap();
        assert!(room.participants[0].paused, "{body}");
    }
}

#[test]
fn peer_addresses_respect_manual_pause_provider_waits_room_pause_and_turn_limit() {
    for (state, pending, exhausted, room_paused) in [
        ("paused", false, false, false),
        ("waiting", false, false, false),
        ("quota", false, false, false),
        ("failed", false, false, false),
        ("budget_exhausted", false, false, false),
        ("busy", true, false, false),
        ("no_progress", false, true, false),
        ("blocked", false, false, true),
    ] {
        let fixture = Fixture::new();
        fixture
            .store
            .update(&fixture.room.id, |r| {
                r.paused = room_paused;
                let peer = &mut r.participants[0];
                peer.paused = true;
                peer.state = state.into();
                if pending {
                    peer.pending_delivery = Some(delivery("message", None, fixture.now()));
                }
                if exhausted {
                    peer.wake_count = peer.max_turns;
                }
                Ok(())
            })
            .unwrap();
        let room = fixture
            .store
            .append_message(
                &fixture.room.id,
                "Claude",
                "@Codex review".into(),
                Some("peer-b".into()),
                None,
                None,
            )
            .unwrap();
        assert!(room.participants[0].paused, "{state}");
        assert_eq!(room.participants[0].state, state);
    }
}

#[test]
fn peer_everyone_is_scoped_to_sidechat_members() {
    let fixture = Fixture::new();
    let source = fixture
        .store
        .post_message(&fixture.room.id, "Source".into())
        .unwrap()
        .messages[0]
        .id
        .clone();
    let room = fixture
        .store
        .create_sidechat(
            &fixture.room.id,
            &source,
            "Review pair",
            vec!["peer-a".into(), "peer-b".into()],
        )
        .unwrap();
    fixture
        .store
        .update(&room.id, |r| {
            r.auto_continue = false;
            r.participants.push(participant("outside", "Outside"));
            for peer in &mut r.participants {
                peer.message_cursor = r.messages.len();
                if peer.id != "peer-b" {
                    peer.paused = true;
                    peer.state = "blocked".into();
                }
            }
            Ok(())
        })
        .unwrap();
    let scoped = fixture
        .store
        .append_message_in_sidechat(
            &room.id,
            "Claude",
            "@everyone please review".into(),
            Some("peer-b".into()),
            None,
            None,
            Some(room.sidechats[0].id.clone()),
        )
        .unwrap();
    assert!(!scoped.participants[0].paused);
    assert!(scoped.participants[2].paused);
    assert_eq!(
        scoped
            .messages
            .last()
            .unwrap()
            .target_participant_id
            .as_deref(),
        Some("peer-a")
    );
    assert!(plan(&scoped, &scoped.participants[1], fixture.now()).is_none());
}

#[test]
fn queued_peer_input_survives_a_finishing_blocked_delivery_and_reopen() {
    let fixture = Fixture::new();
    fixture
        .store
        .update(&fixture.room.id, |r| {
            r.auto_continue = false;
            let peer = &mut r.participants[0];
            peer.paused = true;
            peer.state = "blocked".into();
            peer.pending_delivery = Some(delivery("task", None, fixture.now()));
            Ok(())
        })
        .unwrap();
    let queued = fixture
        .store
        .append_message(
            &fixture.room.id,
            "Claude",
            "New review after blocker".into(),
            Some("peer-b".into()),
            Some("peer-a".into()),
            None,
        )
        .unwrap();
    assert!(queued.participants[0].paused);
    let reopened = RoomStore::open(&fixture._temp.path().join("rooms.sqlite3")).unwrap();
    let ready = reopened
        .update(&queued.id, |r| {
            r.participants[0].pending_delivery = None;
            super::wake_dormant_for_unread_messages(r);
            Ok(())
        })
        .unwrap();
    assert!(!ready.participants[0].paused);
    assert_eq!(
        plan(&ready, &ready.participants[0], fixture.now())
            .unwrap()
            .reason,
        "message"
    );
}

#[test]
fn everyone_does_not_revive_a_paused_room_or_nonmembers_of_a_sidechat() {
    let fixture = Fixture::new();
    let source = fixture
        .store
        .post_message(&fixture.room.id, "Discuss here".into())
        .unwrap()
        .messages[0]
        .id
        .clone();
    let room = fixture
        .store
        .create_sidechat(
            &fixture.room.id,
            &source,
            "Codex only",
            vec!["peer-a".into()],
        )
        .unwrap();
    fixture
        .store
        .update(&room.id, |r| {
            for peer in &mut r.participants {
                peer.paused = true;
                peer.state = "blocked".into();
                peer.message_cursor = r.messages.len();
            }
            Ok(())
        })
        .unwrap();
    let scoped = fixture
        .store
        .append_message_in_sidechat(
            &room.id,
            "Human",
            "@everyone respond".into(),
            None,
            None,
            None,
            Some(room.sidechats[0].id.clone()),
        )
        .unwrap();
    assert!(!scoped.participants[0].paused);
    assert!(scoped.participants[1].paused);
    fixture
        .store
        .update(&room.id, |r| {
            r.paused = true;
            r.participants[0].paused = true;
            r.participants[0].state = "blocked".into();
            Ok(())
        })
        .unwrap();
    let paused = fixture
        .store
        .post_message(&room.id, "@everyone respond".into())
        .unwrap();
    assert!(paused.participants.iter().all(|p| p.paused));
}

#[test]
fn human_broadcast_and_direct_message_wake_but_untargeted_agent_message_does_not() {
    let fixture = Fixture::new();
    let peer = fixture.peer("peer-a");
    let now = fixture.now();

    let broadcast = fixture
        .store
        .post_message(&fixture.room.id, "Human broadcast".into())
        .unwrap();
    assert_eq!(plan(&broadcast, &peer, now).unwrap().reason, "message");

    let direct = fixture
        .store
        .append_message(
            &fixture.room.id,
            "Human",
            "For peer A".into(),
            None,
            Some(peer.id.clone()),
            None,
        )
        .unwrap();
    let mut after_broadcast = peer.clone();
    after_broadcast.message_cursor = broadcast.messages.len();
    assert_eq!(
        plan(&direct, &after_broadcast, now).unwrap().reason,
        "message"
    );

    let agent_message = fixture
        .store
        .append_message(
            &fixture.room.id,
            "Claude",
            "Peer update".into(),
            Some("peer-b".into()),
            None,
            None,
        )
        .unwrap();
    let mut after_direct = peer;
    after_direct.message_cursor = direct.messages.len();
    let mut no_other_work = agent_message;
    no_other_work.auto_continue = false;
    for item in &mut no_other_work.board.items {
        item.status = "Blocked".into();
    }
    assert!(plan(&no_other_work, &after_direct, now).is_none());
}

#[test]
fn seeded_history_never_replays_as_new_work_for_joining_peers() {
    let fixture = Fixture::new();
    let peer = fixture.peer("peer-b");
    let mut room = fixture.room.clone();
    room.auto_continue = false;
    room.origin = Some(crate::rooms::models::RoomOrigin {
        run_id: "original-run".into(),
        provider: "codex".into(),
        session_id: "original-thread".into(),
        title: "Earlier work".into(),
        message_count: 1,
        context: "Earlier request".into(),
    });
    room.messages.push(crate::rooms::models::Message {
        id: "historical-message".into(),
        sender: "Human".into(),
        body: "An old request that must not be replayed".into(),
        created_at: crate::models::now_iso(),
        participant_id: None,
        target_participant_id: None,
        target_participant_ids: vec![],
        source_event_id: Some("original-run:12".into()),
        sidechat_id: None,
        attachments: vec![],
    });
    assert!(plan(&room, &peer, fixture.now()).is_none());
    let mut new_message = room.messages[0].clone();
    new_message.id = "new-message".into();
    new_message.source_event_id = None;
    new_message.body = "A new request".into();
    room.messages.push(new_message);
    assert_eq!(plan(&room, &peer, fixture.now()).unwrap().reason, "message");
}

#[test]
fn paused_archived_busy_waiting_pending_and_budget_exhausted_peers_never_plan() {
    let fixture = Fixture::new();
    let peer = fixture.peer("peer-a");
    let now = fixture.now();
    let mut room = fixture.room.clone();
    room.paused = true;
    assert!(plan(&room, &peer, now).is_none());
    room.paused = false;
    room.archived = true;
    assert!(plan(&room, &peer, now).is_none());
    room.archived = false;

    let mut paused = peer.clone();
    paused.paused = true;
    assert!(plan(&room, &paused, now).is_none());
    for state in ["busy", "waiting"] {
        let mut busy = peer.clone();
        busy.state = state.into();
        assert!(plan(&room, &busy, now).is_none());
    }
    let mut pending = peer.clone();
    pending.pending_delivery = Some(delivery("message", None, now));
    assert!(plan(&room, &pending, now).is_none());
    let mut exhausted = peer.clone();
    exhausted.wake_count = exhausted.max_turns;
    assert!(plan(&room, &exhausted, now).is_none());
}

#[test]
fn concurrent_limit_is_checked_during_planning_and_atomic_reservation() {
    let mut fixture = Fixture::new();
    fixture.room = fixture
        .store
        .update(&fixture.room.id, |room| {
            room.max_concurrent = 1;
            Ok(())
        })
        .unwrap();
    let now = fixture.now();
    let peer_a = fixture.peer("peer-a");
    let peer_b = fixture.peer("peer-b");
    let planned_a = plan(&fixture.room, &peer_a, now).unwrap();
    let planned_b = plan(&fixture.room, &peer_b, now).unwrap();

    reserve_delivery(&fixture.store, &fixture.room.id, &peer_a.id, planned_a, now).unwrap();
    assert!(
        reserve_delivery(&fixture.store, &fixture.room.id, &peer_b.id, planned_b, now).is_err()
    );
    let after = fixture.store.get(&fixture.room.id).unwrap();
    assert_eq!(
        after
            .participants
            .iter()
            .filter(|p| p.pending_delivery.is_some())
            .count(),
        1
    );
    assert!(plan(&after, &peer_b, now).is_none());
}

#[test]
fn parallel_delivery_reservations_never_exceed_room_limit() {
    let mut fixture = Fixture::new();
    fixture.room = fixture
        .store
        .update(&fixture.room.id, |room| {
            room.max_concurrent = 1;
            Ok(())
        })
        .unwrap();
    let path = fixture._temp.path().join("rooms.sqlite3");
    let room_id = fixture.room.id.clone();
    let barrier = Arc::new(Barrier::new(3));
    let mut joins = Vec::new();
    for (peer_id, created_at) in [("peer-a", 100), ("peer-b", 101)] {
        let peer_id = peer_id.to_owned();
        let path = path.clone();
        let room_id = room_id.clone();
        let barrier = barrier.clone();
        joins.push(std::thread::spawn(move || {
            let store = RoomStore::open(Path::new(&path)).unwrap();
            barrier.wait();
            reserve_delivery(
                &store,
                &room_id,
                &peer_id,
                delivery("message", None, created_at),
                created_at,
            )
            .is_ok()
        }));
    }
    barrier.wait();
    let outcomes = joins
        .into_iter()
        .map(|join| join.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(outcomes.iter().filter(|won| **won).count(), 1);
    assert_eq!(outcomes.iter().filter(|won| !**won).count(), 1);
    let after = fixture.store.get(&room_id).unwrap();
    assert_eq!(
        after
            .participants
            .iter()
            .filter(|p| p.pending_delivery.is_some())
            .count(),
        1
    );
}

#[test]
fn busy_timer_is_durably_coalesced_and_delivered_once_after_long_gap() {
    let fixture = Fixture::new();
    let now = fixture.now();
    fixture
        .store
        .update(&fixture.room.id, |room| {
            let peer = room
                .participants
                .iter_mut()
                .find(|p| p.id == "peer-a")
                .unwrap();
            peer.state = "busy".into();
            peer.pending_delivery = Some(delivery("task", None, now - 60_000));
            room.timers.push(timer("timer-a", "peer-a", now - 10));
            room.timers[0].idle_only = false;
            Ok(())
        })
        .unwrap();

    queue_due_timers(&fixture.store, &fixture.room.id, "peer-a", now).unwrap();
    let first_queue = fixture.store.get(&fixture.room.id).unwrap().timers[0]
        .queued_at
        .unwrap();
    queue_due_timers(&fixture.store, &fixture.room.id, "peer-a", now + 600_000).unwrap();
    let queued = fixture.store.get(&fixture.room.id).unwrap();
    assert_eq!(queued.timers[0].queued_at, Some(first_queue));
    assert_eq!(queued.timers[0].delivered_count, 0);

    let idle = fixture
        .store
        .update(&fixture.room.id, |room| {
            let peer = room
                .participants
                .iter_mut()
                .find(|p| p.id == "peer-a")
                .unwrap();
            peer.state = "idle".into();
            peer.pending_delivery = None;
            Ok(())
        })
        .unwrap();
    let peer = idle.participants.iter().find(|p| p.id == "peer-a").unwrap();
    let planned = plan(&idle, peer, now + 600_000).unwrap();
    assert_eq!(planned.timer_id.as_deref(), Some("timer-a"));
    let planned_id = planned.id.clone();
    let reserved = reserve_delivery(
        &fixture.store,
        &fixture.room.id,
        "peer-a",
        planned,
        now + 600_000,
    )
    .unwrap();
    assert_eq!(reserved.timers[0].delivered_count, 0);
    let delivered = fixture
        .store
        .update(&fixture.room.id, |room| {
            super::mark_delivery_accepted(room, "peer-a", &planned_id, now + 600_000)
        })
        .unwrap();
    assert_eq!(delivered.timers[0].delivered_count, 1);
    assert_eq!(delivered.timers[0].queued_at, None);
    assert_eq!(delivered.timers[0].next_due_at, now + 630_000);
}

#[test]
fn expired_date_timer_queued_while_busy_is_never_planned_reserved_or_dispatched() {
    let fixture = Fixture::new();
    let now = fixture.now();
    fixture
        .store
        .update(&fixture.room.id, |room| {
            let mut timer = timer("timer-a", "peer-a", now - 1);
            timer.idle_only = false;
            timer.max_deliveries = None;
            timer.ends_at = Some(now + 100);
            room.timers.push(timer);
            let peer = room
                .participants
                .iter_mut()
                .find(|peer| peer.id == "peer-a")
                .unwrap();
            peer.state = "busy".into();
            peer.pending_delivery = Some(delivery("task", None, now - 10));
            Ok(())
        })
        .unwrap();

    queue_due_timers(&fixture.store, &fixture.room.id, "peer-a", now).unwrap();
    assert_eq!(
        fixture.store.get(&fixture.room.id).unwrap().timers[0].queued_at,
        Some(now)
    );

    // A permission wait can outlive the due timer. The queued delivery expires while the peer
    // is waiting and must be cleared before that peer returns to idle.
    fixture
        .store
        .update(&fixture.room.id, |room| {
            let peer = room
                .participants
                .iter_mut()
                .find(|peer| peer.id == "peer-a")
                .unwrap();
            peer.state = "waiting".into();
            Ok(())
        })
        .unwrap();
    queue_due_timers(&fixture.store, &fixture.room.id, "peer-a", now + 100).unwrap();
    let expired_while_waiting = fixture.store.get(&fixture.room.id).unwrap();
    assert_eq!(expired_while_waiting.timers[0].queued_at, None);

    let idle = fixture
        .store
        .update(&fixture.room.id, |room| {
            room.auto_continue = false;
            for item in &mut room.board.items {
                item.status = "Blocked".into();
            }
            let peer = room
                .participants
                .iter_mut()
                .find(|peer| peer.id == "peer-a")
                .unwrap();
            peer.state = "idle".into();
            peer.pending_delivery = None;
            Ok(())
        })
        .unwrap();
    let peer = idle
        .participants
        .iter()
        .find(|peer| peer.id == "peer-a")
        .unwrap();
    assert!(plan(&idle, peer, now + 100).is_none());

    let stale_delivery = delivery("timer", Some("timer-a"), now + 100);
    assert!(reserve_delivery(
        &fixture.store,
        &fixture.room.id,
        "peer-a",
        stale_delivery,
        now + 100,
    )
    .is_err());
    assert!(!super::timer_delivery_is_dispatchable(
        &idle,
        "peer-a",
        "timer-a",
        now + 100,
    ));
    assert_eq!(idle.timers[0].delivered_count, 0);
}

#[test]
fn dispatch_preflight_cancels_a_timer_that_expires_after_reservation() {
    let fixture = Fixture::new();
    let now = fixture.now();
    let room = fixture
        .store
        .update(&fixture.room.id, |room| {
            room.auto_continue = false;
            for item in &mut room.board.items {
                item.status = "Blocked".into();
            }
            let mut timer = timer("timer-a", "peer-a", now - 1);
            timer.max_deliveries = None;
            timer.ends_at = Some(now + 100);
            room.timers.push(timer);
            Ok(())
        })
        .unwrap();
    let peer = room
        .participants
        .iter()
        .find(|peer| peer.id == "peer-a")
        .unwrap();
    let planned = plan(&room, peer, now).unwrap();
    let reserved = reserve_delivery(&fixture.store, &room.id, &peer.id, planned, now).unwrap();
    assert_eq!(reserved.timers[0].delivered_count, 0);
    assert!(reserved.participants[0].pending_delivery.is_some());

    // The first preflight succeeds before a cold actor startup. Simulate startup lasting
    // through the timer deadline, then run the same production preflight used immediately
    // before enqueueing. At the inclusive end boundary, no payload is returned for delivery.
    assert!(
        super::prepare_dispatch(&fixture.store, &room.id, "peer-a", now)
            .unwrap()
            .is_some()
    );
    assert!(
        super::prepare_dispatch(&fixture.store, &room.id, "peer-a", now + 100)
            .unwrap()
            .is_none()
    );
    let after = fixture.store.get(&room.id).unwrap();
    assert_eq!(after.timers[0].delivered_count, 0);
    assert!(after.participants[0].pending_delivery.is_none());
    assert_eq!(after.participants[0].wake_count, 0);
}

#[test]
fn waiting_paused_and_idle_only_busy_peers_do_not_queue_timers() {
    let fixture = Fixture::new();
    let now = fixture.now();
    fixture
        .store
        .update(&fixture.room.id, |room| {
            room.timers.push(timer("timer-a", "peer-a", now - 10));
            room.timers[0].idle_only = false;
            let peer = room
                .participants
                .iter_mut()
                .find(|p| p.id == "peer-a")
                .unwrap();
            peer.state = "waiting".into();
            Ok(())
        })
        .unwrap();
    queue_due_timers(&fixture.store, &fixture.room.id, "peer-a", now).unwrap();
    assert_eq!(
        fixture.store.get(&fixture.room.id).unwrap().timers[0].queued_at,
        None
    );

    fixture
        .store
        .update(&fixture.room.id, |room| {
            let peer = room
                .participants
                .iter_mut()
                .find(|p| p.id == "peer-a")
                .unwrap();
            peer.state = "busy".into();
            room.timers[0].idle_only = true;
            Ok(())
        })
        .unwrap();
    queue_due_timers(&fixture.store, &fixture.room.id, "peer-a", now).unwrap();
    assert_eq!(
        fixture.store.get(&fixture.room.id).unwrap().timers[0].queued_at,
        None
    );

    fixture
        .store
        .update(&fixture.room.id, |room| {
            room.paused = true;
            room.timers[0].idle_only = false;
            Ok(())
        })
        .unwrap();
    queue_due_timers(&fixture.store, &fixture.room.id, "peer-a", now).unwrap();
    assert_eq!(
        fixture.store.get(&fixture.room.id).unwrap().timers[0].queued_at,
        None
    );
}

#[test]
fn task_turns_pause_after_three_unchanged_work_fingerprints() {
    let mut peer = participant("peer", "Codex");
    peer.work_signature = Some("same-work".into());
    for completed in 1u32..=3 {
        peer.state = "busy".into();
        peer.pending_delivery = Some(delivery("task", None, completed as i64));
        super::apply_event_state(
            &mut peer,
            "run_state",
            &serde_json::json!({"state":"completed"}),
            Some("same-work"),
        );
        assert_eq!(peer.no_progress_turns, completed);
    }
    assert!(peer.paused);
    assert_eq!(peer.state, "no_progress");
    assert!(peer
        .last_error
        .as_deref()
        .unwrap()
        .contains("3 completed task turns"));

    let mut progressed = participant("peer", "Codex");
    progressed.work_signature = Some("before".into());
    progressed.no_progress_turns = 2;
    progressed.pending_delivery = Some(delivery("task", None, 1));
    super::apply_event_state(
        &mut progressed,
        "run_state",
        &serde_json::json!({"state":"completed"}),
        Some("after"),
    );
    assert_eq!(progressed.no_progress_turns, 0);
}

#[test]
fn waiting_permission_and_quota_events_have_explicit_state() {
    let mut peer = participant("peer", "Codex");
    peer.state = "busy".into();
    peer.pending_delivery = Some(delivery("task", None, 1));
    super::apply_event_state(&mut peer, "permission_prompt", &serde_json::json!({}), None);
    assert_eq!(peer.state, "waiting");
    assert!(peer
        .last_error
        .as_deref()
        .unwrap()
        .contains("Waiting for human input"));

    super::apply_event_state(
        &mut peer,
        "interaction_resolved",
        &serde_json::json!({}),
        None,
    );
    assert_eq!(peer.state, "busy");
    assert_eq!(peer.last_error, None);

    super::apply_event_state(
        &mut peer,
        "rate_limit_event",
        &serde_json::json!({"status":"rejected"}),
        None,
    );
    assert!(peer.paused);
    assert_eq!(peer.state, "quota");
    assert!(peer
        .last_error
        .as_deref()
        .unwrap()
        .contains("Provider quota rejected"));
}

#[test]
fn timer_reservation_consumes_delivery_budget_only_after_acceptance() {
    let fixture = Fixture::new();
    let peer = fixture.peer("peer-a");
    let now = fixture.now();
    fixture
        .store
        .update(&fixture.room.id, |room| {
            room.timers.push(timer("timer-a", &peer.id, now));
            Ok(())
        })
        .unwrap();
    let timer_delivery = delivery("timer", Some("timer-a"), now);
    let reserved = reserve_delivery(
        &fixture.store,
        &fixture.room.id,
        &peer.id,
        timer_delivery.clone(),
        now,
    )
    .unwrap();
    assert_eq!(reserved.timers[0].delivered_count, 0);
    assert_eq!(reserved.participants[0].wake_count, 1);
    assert_eq!(
        reserved.participants[0]
            .pending_delivery
            .as_ref()
            .unwrap()
            .state,
        "prepared"
    );

    let duplicate = reserve_delivery(
        &fixture.store,
        &fixture.room.id,
        &peer.id,
        timer_delivery.clone(),
        now,
    );
    assert!(duplicate.is_err());
    let after = fixture.store.get(&fixture.room.id).unwrap();
    assert_eq!(after.timers[0].delivered_count, 0);
    assert_eq!(after.participants[0].wake_count, 1);

    let accepted = fixture
        .store
        .update(&fixture.room.id, |room| {
            super::mark_delivery_accepted(room, &peer.id, &timer_delivery.id, now + 1)
        })
        .unwrap();
    assert_eq!(accepted.timers[0].delivered_count, 1);
    assert_eq!(accepted.timers[0].next_due_at, now + 30_001);
}

#[test]
fn timer_edit_between_planning_and_reservation_uses_current_recipient_and_message() {
    let fixture = Fixture::new();
    let now = fixture.now();
    fixture
        .store
        .update(&fixture.room.id, |r| {
            r.timers.push(timer("timer-a", "peer-b", now));
            Ok(())
        })
        .unwrap();
    let planned = delivery("timer", Some("timer-a"), now);
    assert!(reserve_delivery(
        &fixture.store,
        &fixture.room.id,
        "peer-a",
        planned.clone(),
        now
    )
    .is_err());
    let after = fixture.store.get(&fixture.room.id).unwrap();
    assert_eq!(after.timers[0].delivered_count, 0);
    assert_eq!(after.participants[0].wake_count, 0);

    fixture
        .store
        .update(&fixture.room.id, |r| {
            r.timers[0].participant_id = "peer-a".into();
            r.timers[0].message = "EDITED_TIMER_MESSAGE".into();
            Ok(())
        })
        .unwrap();
    let reserved =
        reserve_delivery(&fixture.store, &fixture.room.id, "peer-a", planned, now).unwrap();
    assert!(reserved.participants[0]
        .pending_delivery
        .as_ref()
        .unwrap()
        .text
        .contains("EDITED_TIMER_MESSAGE"));
}

#[test]
fn recovery_pauses_prepared_or_sent_intents_without_replay() {
    for intent_state in ["prepared", "sent"] {
        let fixture = Fixture::new();
        let now = fixture.now();
        let mut intent = delivery("message", None, now);
        intent.state = intent_state.into();
        fixture
            .store
            .update(&fixture.room.id, |room| {
                let peer = room
                    .participants
                    .iter_mut()
                    .find(|peer| peer.id == "peer-a")
                    .unwrap();
                peer.state = "busy".into();
                peer.pending_delivery = Some(intent);
                Ok(())
            })
            .unwrap();
        recover(&fixture.store).unwrap();
        let recovered = fixture.store.get(&fixture.room.id).unwrap();
        let peer = recovered
            .participants
            .iter()
            .find(|peer| peer.id == "peer-a")
            .unwrap();
        assert!(peer.paused);
        assert_eq!(peer.state, "waiting");
        assert_eq!(peer.pending_delivery.as_ref().unwrap().state, intent_state);
        assert!(plan(&recovered, peer, now).is_none());
    }
}

#[test]
fn parallel_sqlite_reservations_have_one_winner_and_peer_cannot_reserve_another_task() {
    let fixture = Fixture::new();
    let path = fixture._temp.path().join("rooms.sqlite3");
    let room_id = fixture.room.id.clone();
    let barrier = Arc::new(Barrier::new(3));
    let mut joins = Vec::new();
    for peer_id in ["peer-a", "peer-b"] {
        let peer_id = peer_id.to_owned();
        let path = path.clone();
        let room_id = room_id.clone();
        let barrier = barrier.clone();
        joins.push(std::thread::spawn(move || {
            let store = RoomStore::open(Path::new(&path)).unwrap();
            barrier.wait();
            store
                .reserve_claim(&room_id, &peer_id, "task-a")
                .map(|_| peer_id)
        }));
    }
    barrier.wait();
    let outcomes = joins
        .into_iter()
        .map(|join| join.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
    assert_eq!(
        outcomes.iter().filter(|outcome| outcome.is_err()).count(),
        1
    );
    let winner = outcomes.into_iter().find_map(Result::ok).unwrap();
    assert!(fixture
        .store
        .reserve_claim(&room_id, &winner, "task-b")
        .is_err());
}

#[test]
fn duplicate_source_event_messages_are_appended_only_once() {
    let fixture = Fixture::new();
    fixture
        .store
        .append_message(
            &fixture.room.id,
            "Codex",
            "One event".into(),
            Some("peer-a".into()),
            None,
            Some("event-1".into()),
        )
        .unwrap();
    let second = fixture
        .store
        .append_message(
            &fixture.room.id,
            "Codex",
            "Duplicate event".into(),
            Some("peer-a".into()),
            None,
            Some("event-1".into()),
        )
        .unwrap();
    assert_eq!(
        second
            .messages
            .iter()
            .filter(|message| message.source_event_id.as_deref() == Some("event-1"))
            .count(),
        1
    );
}

#[test]
fn room_tool_and_transcript_projection_dedupe_per_delivery() {
    let fixture = Fixture::new();
    let room_id = &fixture.room.id;
    let source_for = |delivery_id: &str, target: Option<&str>| {
        crate::rooms::message_projection::delivery_source_event_id(
            "run-peer-a",
            delivery_id,
            "peer-a",
            None,
            target,
            "Claude update",
        )
        .unwrap()
    };

    // MCP post_message and the later message_complete transcript event share
    // the source identity for one room delivery.
    let first_source = source_for("delivery-1", None);
    fixture
        .store
        .append_message(
            room_id,
            "Claude",
            "Claude update".into(),
            Some("peer-a".into()),
            None,
            Some(first_source.clone()),
        )
        .unwrap();
    fixture
        .store
        .append_message(
            room_id,
            "Claude",
            "Claude update".into(),
            Some("peer-a".into()),
            None,
            Some(first_source.clone()),
        )
        .unwrap();

    // Identical text from a later delivery remains a separate message.
    let second_source = source_for("delivery-2", None);
    fixture
        .store
        .append_message(
            room_id,
            "Claude",
            "Claude update".into(),
            Some("peer-a".into()),
            None,
            Some(second_source.clone()),
        )
        .unwrap();

    // A targeted post has a different identity from an unaddressed transcript
    // message so recipient distinctions are not collapsed.
    let targeted_source = source_for("delivery-1", Some("peer-b"));
    fixture
        .store
        .append_message(
            room_id,
            "Claude",
            "Claude update".into(),
            Some("peer-a".into()),
            Some("peer-b".into()),
            Some(targeted_source.clone()),
        )
        .unwrap();

    let saved = fixture.store.get(room_id).unwrap();
    let projected = saved
        .messages
        .iter()
        .filter(|message| message.body == "Claude update")
        .collect::<Vec<_>>();
    assert_eq!(projected.len(), 3);
    assert!(projected
        .iter()
        .any(|message| message.source_event_id.as_deref() == Some(first_source.as_str())));
    assert!(projected
        .iter()
        .any(|message| message.source_event_id.as_deref() == Some(second_source.as_str())));
    assert!(projected.iter().any(|message| {
        message.source_event_id.as_deref() == Some(targeted_source.as_str())
            && message.target_participant_id.as_deref() == Some("peer-b")
    }));
}

#[test]
fn explicit_public_post_suppresses_a_paraphrased_transcript_echo_only_for_its_delivery() {
    let fixture = Fixture::new();
    fixture
        .store
        .update(&fixture.room.id, |room| {
            room.participants[0].pending_delivery = Some(delivery("message", None, 1));
            Ok(())
        })
        .unwrap();
    let room = fixture.store.get(&fixture.room.id).unwrap();
    let pending = room.participants[0].pending_delivery.as_ref().unwrap();
    let posted = "roundtrip nonce Q7M4";
    let source = crate::rooms::message_projection::delivery_source_event_id(
        &room.participants[0].run_id,
        &pending.id,
        &room.participants[0].id,
        None,
        None,
        posted,
    )
    .unwrap();
    fixture
        .store
        .append_message(
            &room.id,
            "Claude",
            posted.into(),
            Some(room.participants[0].id.clone()),
            None,
            Some(source),
        )
        .unwrap();
    let before_echo = fixture.store.get(&room.id).unwrap();
    import_message_complete(
        &fixture,
        1,
        "provider-message-1",
        "I posted the roundtrip nonce Q7M4 to the public room.",
    );
    let after_echo = fixture.store.get(&room.id).unwrap();
    assert_eq!(after_echo.messages.len(), before_echo.messages.len());
    assert!(after_echo
        .messages
        .iter()
        .any(|message| message.body == posted));

    // The ownership check is delivery-scoped, so an identical later message
    // still appears when its delivery made no public room post.
    fixture
        .store
        .update(&room.id, |room| {
            room.participants[0].pending_delivery = Some(delivery("message", None, 2));
            Ok(())
        })
        .unwrap();
    import_message_complete(
        &fixture,
        2,
        "provider-message-2",
        "I posted roundtrip nonce Q7M4.",
    );
    let after_later_delivery = fixture.store.get(&room.id).unwrap();
    assert_eq!(
        after_later_delivery.messages.len(),
        before_echo.messages.len() + 1
    );
    assert!(after_later_delivery
        .messages
        .iter()
        .any(|message| message.body == "I posted roundtrip nonce Q7M4."));
}

#[test]
fn directed_main_and_sidechat_posts_do_not_suppress_transcript_projection() {
    let fixture = Fixture::new();
    let root = fixture
        .store
        .append_message(
            &fixture.room.id,
            "Human",
            "Sidechat source".into(),
            None,
            None,
            None,
        )
        .unwrap()
        .messages
        .last()
        .unwrap()
        .id
        .clone();
    let room = fixture
        .store
        .create_sidechat(
            &fixture.room.id,
            &root,
            "Focused discussion",
            vec!["peer-a".into(), "peer-b".into()],
        )
        .unwrap();
    let sidechat_id = room.sidechats[0].id.clone();
    fixture
        .store
        .update(&room.id, |room| {
            room.participants[0].pending_delivery = Some(delivery("message", None, 10));
            Ok(())
        })
        .unwrap();
    let room = fixture.store.get(&room.id).unwrap();
    let peer = &room.participants[0];
    let directed_body = "Directed status for peer B";
    let directed_source = crate::rooms::message_projection::delivery_source_event_id(
        &peer.run_id,
        &peer.pending_delivery.as_ref().unwrap().id,
        &peer.id,
        None,
        Some("peer-b"),
        directed_body,
    )
    .unwrap();
    fixture
        .store
        .append_message(
            &room.id,
            &peer.name,
            directed_body.into(),
            Some(peer.id.clone()),
            Some("peer-b".into()),
            Some(directed_source),
        )
        .unwrap();
    import_message_complete(
        &fixture,
        1,
        "provider-message-directed",
        "I sent peer B the directed status.",
    );

    fixture
        .store
        .update(&room.id, |room| {
            room.participants[0].pending_delivery = Some(delivery("message", None, 20));
            room.participants[0]
                .pending_delivery
                .as_mut()
                .unwrap()
                .sidechat_id = Some(sidechat_id.clone());
            room.participants[0].active_sidechat_id = Some(sidechat_id.clone());
            Ok(())
        })
        .unwrap();
    let room = fixture.store.get(&room.id).unwrap();
    let peer = &room.participants[0];
    let sidechat_body = "Sidechat-only status";
    let sidechat_source = crate::rooms::message_projection::delivery_source_event_id(
        &peer.run_id,
        &peer.pending_delivery.as_ref().unwrap().id,
        &peer.id,
        Some(&sidechat_id),
        None,
        sidechat_body,
    )
    .unwrap();
    fixture
        .store
        .append_message_in_sidechat(
            &room.id,
            &peer.name,
            sidechat_body.into(),
            Some(peer.id.clone()),
            None,
            Some(sidechat_source),
            Some(sidechat_id.clone()),
        )
        .unwrap();
    let before_sidechat_echo = fixture.store.get(&room.id).unwrap();
    import_message_complete(
        &fixture,
        2,
        "provider-message-sidechat",
        "I added the sidechat-only status in the focused discussion.",
    );
    let saved = fixture.store.get(&room.id).unwrap();
    assert_eq!(
        saved.messages.len(),
        before_sidechat_echo.messages.len() + 1
    );
    let transcript = saved.messages.last().unwrap();
    assert!(transcript.body.contains("focused discussion"));
    assert_eq!(
        transcript.sidechat_id.as_deref(),
        Some(sidechat_id.as_str())
    );
}

#[test]
fn repeated_provider_message_ids_dedupe_transcript_projection() {
    let fixture = Fixture::new();
    let source = crate::rooms::message_projection::provider_message_source_event_id(
        "run-peer-a",
        "provider-message-1",
    )
    .unwrap();
    fixture
        .store
        .append_message(
            &fixture.room.id,
            "Claude",
            "First projection".into(),
            Some("peer-a".into()),
            None,
            Some(source.clone()),
        )
        .unwrap();
    fixture
        .store
        .append_message(
            &fixture.room.id,
            "Claude",
            "Repeated event projection".into(),
            Some("peer-a".into()),
            None,
            Some(source.clone()),
        )
        .unwrap();
    let later_source = crate::rooms::message_projection::provider_message_source_event_id(
        "run-peer-a",
        "provider-message-2",
    )
    .unwrap();
    let saved = fixture
        .store
        .append_message(
            &fixture.room.id,
            "Claude",
            "First projection".into(),
            Some("peer-a".into()),
            None,
            Some(later_source),
        )
        .unwrap();

    assert_eq!(saved.messages.len(), fixture.room.messages.len() + 2);
    assert_eq!(
        saved
            .messages
            .iter()
            .filter(|message| message.source_event_id.as_deref() == Some(source.as_str()))
            .count(),
        1
    );
}

#[test]
fn deduplicated_room_tool_messages_keep_mention_routing_and_dormant_wakeups() {
    let fixture = Fixture::new();
    fixture
        .store
        .update(&fixture.room.id, |room| {
            room.participants.push(participant("lead", "Lead"));
            for peer in &mut room.participants[..2] {
                peer.paused = true;
                peer.state = "blocked".into();
            }
            Ok(())
        })
        .unwrap();

    let body = "@everyone please review this";
    let source = crate::rooms::message_projection::delivery_source_event_id(
        "run-lead",
        "delivery-lead-1",
        "lead",
        None,
        None,
        body,
    )
    .unwrap();
    let saved = fixture
        .store
        .append_message(
            &fixture.room.id,
            "Lead",
            body.into(),
            Some("lead".into()),
            None,
            Some(source),
        )
        .unwrap();

    let routed = saved.messages.last().unwrap();
    assert_eq!(routed.target_participant_ids, vec!["peer-a", "peer-b"]);
    assert!(saved
        .participants
        .iter()
        .filter(|peer| peer.id == "peer-a" || peer.id == "peer-b")
        .all(|peer| !peer.paused && peer.state == "idle"));
}

#[test]
fn parallel_task_creation_intents_have_one_winner_and_reject_body_collision() {
    let fixture = Fixture::new();
    let path = fixture._temp.path().join("rooms.sqlite3");
    let room_id = fixture.room.id.clone();
    let barrier = Arc::new(Barrier::new(3));
    let mut joins = Vec::new();
    for _ in 0..2 {
        let path = path.clone();
        let room_id = room_id.clone();
        let barrier = barrier.clone();
        joins.push(std::thread::spawn(move || {
            let store = RoomStore::open(&path).unwrap();
            barrier.wait();
            store
                .begin_task_creation(&room_id, "Shared title", "Same body")
                .unwrap()
        }));
    }
    barrier.wait();
    let results = joins
        .into_iter()
        .map(|join| join.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|won| **won).count(), 1);
    assert_eq!(results.iter().filter(|won| !**won).count(), 1);

    let second_connection = RoomStore::open(&path).unwrap();
    assert!(second_connection
        .begin_task_creation(&room_id, "Shared title", "Different body")
        .is_err());
}

#[test]
fn unresolved_owned_claim_does_not_start_other_work_after_explicit_resume() {
    for state in ["blocked", "uncertain", "completing", "releasing"] {
        let mut fixture = Fixture::new();
        fixture.room.claims.push(claim("task-a", "peer-a", state));
        let peer = fixture.peer("peer-a");
        assert!(plan(&fixture.room, &peer, fixture.now()).is_none());
    }
}

#[test]
fn reservation_captures_current_recipient_history_without_acknowledging_future_messages() {
    let fixture = Fixture::new();
    // Planning happens before these messages arrive.
    let planned = delivery("message", None, fixture.now());
    fixture
        .store
        .append_message(
            &fixture.room.id,
            "Human",
            "FOR_A_BEFORE_RESERVATION".into(),
            None,
            Some("peer-a".into()),
            None,
        )
        .unwrap();
    for index in 0..25 {
        fixture
            .store
            .append_message(
                &fixture.room.id,
                "Human",
                format!("FOR_B_ONLY_{index}"),
                None,
                Some("peer-b".into()),
                None,
            )
            .unwrap();
    }
    let reserved = reserve_delivery(
        &fixture.store,
        &fixture.room.id,
        "peer-a",
        planned,
        fixture.now(),
    )
    .unwrap();
    let peer = reserved
        .participants
        .iter()
        .find(|p| p.id == "peer-a")
        .unwrap();
    let text = &peer.pending_delivery.as_ref().unwrap().text;
    assert!(text.contains("FOR_A_BEFORE_RESERVATION"));
    assert!(!text.contains("FOR_B_ONLY"));
    assert_eq!(peer.message_cursor, reserved.messages.len());

    let future = fixture
        .store
        .append_message(
            &fixture.room.id,
            "Human",
            "AFTER_RESERVATION".into(),
            None,
            Some("peer-a".into()),
            None,
        )
        .unwrap();
    let peer = future
        .participants
        .iter()
        .find(|p| p.id == "peer-a")
        .unwrap();
    assert_eq!(
        future.messages[peer.message_cursor].body,
        "AFTER_RESERVATION"
    );
}

#[test]
fn delayed_board_refresh_cannot_erase_a_confirmed_task_or_completion() {
    let fixture = Fixture::new();
    let before = fixture.room.board.clone();
    // Model a separate MCP process confirming a new task while the host's read is pending.
    let second = RoomStore::open(&fixture._temp.path().join("rooms.sqlite3")).unwrap();
    let confirmed = second
        .update(&fixture.room.id, |r| {
            r.board.items.push(item("created-task"));
            r.board.items[0].status = "Done".into();
            Ok(())
        })
        .unwrap();
    let mut delayed = before.clone();
    delayed.synced_at = Some("2099-01-01T00:00:00Z".into());
    let retained = fixture
        .store
        .apply_board_snapshot(&fixture.room.id, &before, delayed)
        .unwrap();
    assert_eq!(retained.board, confirmed.board);
    let retained = fixture
        .store
        .apply_board_error(&fixture.room.id, &before, "old request failed".into())
        .unwrap();
    assert_eq!(retained.board, confirmed.board);

    // A read started from the current board can still replace it, including actual removals.
    let mut fresh = confirmed.board.clone();
    fresh.items.retain(|item| item.id != "created-task");
    let applied = fixture
        .store
        .apply_board_snapshot(&fixture.room.id, &confirmed.board, fresh.clone())
        .unwrap();
    assert_eq!(applied.board, fresh);
}

#[test]
fn last_reserved_turn_can_write_governance_but_cannot_start_an_extra_turn() {
    let fixture = Fixture::new();
    let room = fixture
        .store
        .update(&fixture.room.id, |room| {
            room.participants[0].max_turns = 1;
            Ok(())
        })
        .unwrap();
    let planned = plan(&room, &room.participants[0], fixture.now()).unwrap();
    let reserved =
        reserve_delivery(&fixture.store, &room.id, "peer-a", planned, fixture.now()).unwrap();
    assert_eq!(reserved.participants[0].wake_count, 1);
    assert_eq!(
        reserved.participants[0]
            .pending_delivery
            .as_ref()
            .unwrap()
            .state,
        "prepared"
    );
    let question = |title: &str| crate::rooms::models::CreateRequestInput {
        kind: "decision".into(),
        title: title.into(),
        body: "Choose the next approach".into(),
        evidence: None,
        task_id: None,
        reviewer_id: None,
        proposal: None,
        brief: None,
        options: vec![],
    };
    crate::rooms::governance::create_request(
        &fixture.store,
        &room.id,
        "peer-a",
        question("Before acknowledgement"),
    )
    .unwrap();
    fixture
        .store
        .update(&room.id, |room| {
            room.participants[0]
                .pending_delivery
                .as_mut()
                .unwrap()
                .state = "sent".into();
            Ok(())
        })
        .unwrap();
    crate::rooms::governance::create_request(
        &fixture.store,
        &room.id,
        "peer-a",
        question("After acknowledgement"),
    )
    .unwrap();
    let completed = fixture
        .store
        .update(&room.id, |room| {
            room.participants[0].pending_delivery = None;
            room.participants[0].state = "idle".into();
            Ok(())
        })
        .unwrap();
    assert!(crate::rooms::governance::create_request(
        &fixture.store,
        &room.id,
        "peer-a",
        question("Unauthorized extra turn")
    )
    .unwrap_err()
    .contains("budget"));
    assert!(plan(&completed, &completed.participants[0], fixture.now()).is_none());
}

#[test]
fn unlimited_turns_keep_dispatching_and_can_claim_after_many_starts() {
    let fixture = Fixture::new();
    let room = fixture
        .store
        .update(&fixture.room.id, |room| {
            room.auto_continue = true;
            room.participants[0].max_turns = 0;
            room.participants[0].wake_count = 300;
            Ok(())
        })
        .unwrap();
    let peer = &room.participants[0];
    assert!(!peer.turn_limit_reached());
    let next = plan(&room, peer, fixture.now())
        .expect("eligible work should continue without a turn limit");
    let reserved =
        reserve_delivery(&fixture.store, &room.id, &peer.id, next, fixture.now()).unwrap();
    assert_eq!(reserved.participants[0].wake_count, 301);
    fixture
        .store
        .reserve_claim(&room.id, &peer.id, "task-a")
        .unwrap();
}

#[test]
fn unlimited_turns_do_not_bypass_timer_delivery_limits() {
    let fixture = Fixture::new();
    let room = fixture
        .store
        .update(&fixture.room.id, |room| {
            room.auto_continue = false;
            room.participants[0].max_turns = 0;
            room.participants[0].wake_count = 300;
            room.timers.push(Timer {
                id: "limited-timer".into(),
                participant_id: "peer-a".into(),
                message: "Wake".into(),
                interval_seconds: 30,
                idle_only: true,
                enabled: true,
                next_due_at: 1,
                max_deliveries: Some(1),
                ends_at: None,
                delivered_count: 1,
                queued_at: None,
                last_error: None,
            });
            Ok(())
        })
        .unwrap();
    queue_due_timers(&fixture.store, &room.id, "peer-a", fixture.now()).unwrap();
    let current = fixture.store.get(&room.id).unwrap();
    assert!(current.timers[0].is_exhausted_at(fixture.now()));
    assert!(current.timers[0].queued_at.is_none());
    assert!(plan(&current, &current.participants[0], fixture.now()).is_none());
}

#[test]
fn editing_paused_peer_preserves_identity_context_claims_and_turn_count() {
    let fixture = Fixture::new();
    let room = fixture
        .store
        .update(&fixture.room.id, |room| {
            let peer = &mut room.participants[0];
            peer.paused = true;
            peer.state = "budget_exhausted".into();
            peer.max_turns = 1;
            peer.wake_count = 1;
            peer.worktree_path = Some("existing-worktree".into());
            peer.message_cursor = 7;
            peer.event_cursor = 42;
            peer.last_error = Some("Turn budget reached".into());
            room.claims.push(Claim {
                task_id: "task-a".into(),
                participant_id: "peer-a".into(),
                state: "active".into(),
                updated_at: crate::models::now_iso(),
                summary: None,
                evidence: None,
            });
            Ok(())
        })
        .unwrap();
    let original = room.participants[0].clone();
    let input = crate::rooms::models::ParticipantSettings {
        name: "Renamed Codex".into(),
        model: Some("new-account-model".into()),
        effort: Some("high".into()),
        max_turns: 0,
    };
    fixture
        .store
        .update(&room.id, |room| {
            crate::rooms::operations::apply_settings_edit(
                room,
                "peer-a",
                &input,
                &original.settings(),
            )
        })
        .unwrap();
    let reopened = RoomStore::open(&fixture._temp.path().join("rooms.sqlite3")).unwrap();
    let saved = reopened.get(&room.id).unwrap();
    let peer = &saved.participants[0];
    assert_eq!(peer.settings(), input);
    assert_eq!(peer.id, original.id);
    assert_eq!(peer.run_id, original.run_id);
    assert_eq!(peer.worktree_path, original.worktree_path);
    assert_eq!(peer.message_cursor, 7);
    assert_eq!(peer.event_cursor, 42);
    assert_eq!(peer.wake_count, 1);
    assert!(peer.paused);
    assert_eq!(peer.state, "paused");
    assert_eq!(peer.last_error, None);
    assert_eq!(saved.claims.len(), 1);
    assert_eq!(saved.claims[0].state, "active");
    assert_eq!(saved.messages.len(), room.messages.len());
}

#[test]
fn editing_rejects_active_pending_archived_and_stale_settings_without_mutation() {
    let fixture = Fixture::new();
    let input = crate::rooms::models::ParticipantSettings {
        name: "Renamed Codex".into(),
        model: Some("new-model".into()),
        effort: Some("high".into()),
        max_turns: 0,
    };
    for mode in ["unpaused", "busy", "pending", "archived", "stale"] {
        let mut room = fixture.room.clone();
        room.participants[0].paused = true;
        let expected = room.participants[0].settings();
        match mode {
            "unpaused" => room.participants[0].paused = false,
            "busy" => room.participants[0].state = "busy".into(),
            "pending" => {
                room.participants[0].pending_delivery =
                    Some(delivery("message", None, fixture.now()))
            }
            "archived" => room.archived = true,
            "stale" => room.participants[0].model = Some("changed-elsewhere".into()),
            _ => unreachable!(),
        }
        let before = serde_json::to_value(&room).unwrap();
        assert!(
            crate::rooms::operations::apply_settings_edit(&mut room, "peer-a", &input, &expected)
                .is_err(),
            "{mode}"
        );
        assert_eq!(serde_json::to_value(&room).unwrap(), before, "{mode}");
    }
}

#[test]
fn disabling_turn_limit_clears_stale_budget_error_after_room_pause() {
    let fixture = Fixture::new();
    for error in [
        "Turn budget reached. Resume this participant explicitly to grant another budget.",
        "Provider authentication failed",
    ] {
        let mut room = fixture.room.clone();
        let peer = &mut room.participants[0];
        peer.paused = true;
        peer.state = "paused".into();
        peer.max_turns = 1;
        peer.wake_count = 1;
        peer.last_error = Some(error.into());
        let expected = peer.settings();
        let mut input = expected.clone();
        input.max_turns = 0;
        crate::rooms::operations::apply_settings_edit(&mut room, "peer-a", &input, &expected)
            .unwrap();
        assert_eq!(
            room.participants[0].last_error,
            if error.starts_with("Turn budget") {
                None
            } else {
                Some(error.into())
            }
        );
        assert!(room.participants[0].paused);
        assert_eq!(room.participants[0].wake_count, 1);
    }
}

#[test]
fn rename_during_active_turn_preserves_delivery_ownership_and_state() {
    let fixture = Fixture::new();
    fixture
        .store
        .reserve_claim(&fixture.room.id, "peer-a", "task-a")
        .unwrap();
    let room = fixture
        .store
        .update(&fixture.room.id, |room| {
            room.participants[0].state = "busy".into();
            room.participants[0].wake_count = 7;
            room.participants[0].pending_delivery = Some(delivery("message", None, fixture.now()));
            Ok(())
        })
        .unwrap();
    let expected = room.participants[0].settings();
    let mut input = expected.clone();
    input.name = "  Codex builder  ".into();
    let renamed = fixture
        .store
        .update(&room.id, |room| {
            crate::rooms::operations::apply_settings_edit(room, "peer-a", &input, &expected)
        })
        .unwrap();
    let peer = &renamed.participants[0];
    assert_eq!(peer.name, "Codex builder");
    assert!(!peer.paused);
    assert_eq!(peer.state, "busy");
    assert_eq!(peer.wake_count, 7);
    assert_eq!(
        serde_json::to_value(&peer.pending_delivery).unwrap(),
        serde_json::to_value(&room.participants[0].pending_delivery).unwrap()
    );
    assert_eq!(peer.run_id, room.participants[0].run_id);
    assert_eq!(renamed.claims[0].participant_id, "peer-a");
    assert_eq!(
        serde_json::to_value(&renamed.claims).unwrap(),
        serde_json::to_value(&room.claims).unwrap()
    );
}

#[test]
fn rename_rejects_duplicate_empty_long_and_stale_names_without_changes() {
    let fixture = Fixture::new();
    for name in ["  CLAUDE  ".into(), " ".into(), "x".repeat(81)] {
        let mut room = fixture.room.clone();
        let expected = room.participants[0].settings();
        let mut input = expected.clone();
        input.name = name;
        let before = serde_json::to_value(&room).unwrap();
        assert!(crate::rooms::operations::apply_settings_edit(
            &mut room, "peer-a", &input, &expected
        )
        .is_err());
        assert_eq!(serde_json::to_value(&room).unwrap(), before);
    }
    let mut room = fixture.room.clone();
    let expected = room.participants[0].settings();
    let mut input = expected.clone();
    input.name = "Builder".into();
    room.participants[0].name = "Renamed elsewhere".into();
    assert!(
        crate::rooms::operations::apply_settings_edit(&mut room, "peer-a", &input, &expected)
            .unwrap_err()
            .contains("changed elsewhere")
    );
    assert_eq!(room.participants[0].name, "Renamed elsewhere");
}

#[test]
fn mentioned_recipients_receive_attachments_without_leaking_or_acknowledging_later_files() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let f = Fixture::new();
    f.store
        .update(&f.room.id, |r| {
            r.auto_continue = false;
            r.participants.push(participant("peer-c", "Lead"));
            Ok(())
        })
        .unwrap();
    let image = f
        .store
        .upload_attachment(
            &f.room.id,
            "diagram.png",
            &STANDARD.encode(b"\x89PNG\r\n\x1a\nimage bytes"),
        )
        .unwrap();
    let text = f
        .store
        .upload_attachment(
            &f.room.id,
            "notes.txt",
            &STANDARD.encode(b"read these notes"),
        )
        .unwrap();
    let room = f
        .store
        .append_message_with_attachments(
            &f.room.id,
            "Human",
            "@Codex @Claude inspect attachments".into(),
            None,
            Some("peer-c".into()),
            None,
            None,
            &[image.id.clone(), text.id.clone()],
        )
        .unwrap();
    let c = room.participants.iter().find(|p| p.id == "peer-c").unwrap();
    assert!(plan(&room, c, f.now()).is_none());
    assert!(
        !super::prompt(&room, c, &delivery("timer", None, f.now())).contains("inspect attachments")
    );
    let p = room.participants.iter().find(|p| p.id == "peer-a").unwrap();
    let planned = plan(&room, p, f.now()).unwrap();
    let later = f
        .store
        .upload_attachment(&f.room.id, "later.txt", &STANDARD.encode(b"later"))
        .unwrap();
    f.store
        .append_message_with_attachments(
            &f.room.id,
            "Human",
            "Later".into(),
            None,
            Some("peer-a".into()),
            None,
            None,
            &[later.id.clone()],
        )
        .unwrap();
    let reserved = reserve_delivery(&f.store, &f.room.id, &p.id, planned, f.now()).unwrap();
    let pending = reserved
        .participants
        .iter()
        .find(|p| p.id == "peer-a")
        .unwrap()
        .pending_delivery
        .as_ref()
        .unwrap();
    assert_eq!(
        pending.attachment_ids,
        vec![image.id.clone(), text.id.clone()]
    );
    assert!(!pending.attachment_ids.contains(&later.id));
    assert!(pending.text.contains("file-notes.txt"));
    assert!(pending.text.contains("file-diagram.png"));
    let provider = super::provider_attachments(&f.store, &f.room.id, &p.id).unwrap();
    assert_eq!(provider.len(), 1);
    assert_eq!(provider[0].media_type, "image/png");
    assert_eq!(
        STANDARD.decode(&provider[0].content_base64).unwrap(),
        b"\x89PNG\r\n\x1a\nimage bytes"
    );
    let reopened = RoomStore::open(&f._temp.path().join("rooms.sqlite3")).unwrap();
    let saved = reopened.get(&f.room.id).unwrap();
    assert_eq!(
        saved.participants[0]
            .pending_delivery
            .as_ref()
            .unwrap()
            .attachment_ids,
        pending.attachment_ids
    );
}

#[test]
#[ignore = "Local acceptance: isolated copy of active RAV room and its event logs"]
fn live_rav_room_catches_up_past_oversized_tools_without_duplicate_messages() {
    let root = crate::storage::data_dir();
    assert!(
        root.to_string_lossy()
            .contains("ocv-live-reader-acceptance"),
        "Use isolated acceptance profile only"
    );
    let store = RoomStore::open(&root.join("rooms.sqlite3")).unwrap();
    let room_id = "115e121e-9afc-46f9-b7db-aef53900333a";
    let initial = store.get(room_id).unwrap();
    let lead = &initial.participants[0];
    let mut seq = lead.event_cursor;
    let mut offset = None;
    let raw_error = loop {
        match crate::storage::events::list_bus_events_page(&lead.run_id, seq, offset) {
            Ok(page) => {
                assert!(page.has_more);
                seq = page.last_seq;
                offset = Some(page.next_offset);
            }
            Err(e) => break e,
        }
    };
    assert!(raw_error.contains("HISTORY_PROJECTION_REQUIRED"));
    let started = std::time::Instant::now();
    for peer in &initial.participants {
        for _ in 0..1000 {
            let before = store
                .get(room_id)
                .unwrap()
                .participants
                .into_iter()
                .find(|p| p.id == peer.id)
                .unwrap();
            super::import_events(&store, room_id, &before).unwrap();
            let after = store
                .get(room_id)
                .unwrap()
                .participants
                .into_iter()
                .find(|p| p.id == peer.id)
                .unwrap();
            if after.event_offset == before.event_offset
                && after.event_cursor == before.event_cursor
            {
                break;
            }
        }
    }
    let caught_up = store.get(room_id).unwrap();
    for peer in &caught_up.participants {
        assert_eq!(
            peer.event_cursor,
            crate::storage::events::next_seq(&peer.run_id) - 1
        );
        assert!(peer.event_offset.is_some());
        super::import_events(&store, room_id, peer).unwrap();
    }
    let repeated = store.get(room_id).unwrap();
    assert_eq!(caught_up.messages.len(), repeated.messages.len());
    assert!(
        repeated.participants[0].paused,
        "History reconciliation never resumes a paused agent"
    );
    assert_eq!(
        initial.project.as_ref().unwrap().id,
        repeated.project.as_ref().unwrap().id
    );
    eprintln!(
        "Live RAV catch-up: {raw_error}; {} -> {} messages, cursors {:?}, {:?}",
        initial.messages.len(),
        repeated.messages.len(),
        repeated
            .participants
            .iter()
            .map(|p| p.event_cursor)
            .collect::<Vec<_>>(),
        started.elapsed()
    );
}

#[test]
fn unpausing_an_agent_does_not_drop_messages_queued_in_a_paused_room() {
    let fixture = Fixture::new();
    fixture
        .store
        .update(&fixture.room.id, |room| {
            room.paused = true;
            room.participants[0].paused = true;
            Ok(())
        })
        .unwrap();
    let queued = fixture
        .store
        .append_message(
            &fixture.room.id,
            "Human",
            "Investigate the reconnect delay".into(),
            None,
            Some("peer-a".into()),
            None,
        )
        .unwrap();
    let message_id = queued.messages.last().unwrap().id.clone();
    let agent_resumed = fixture
        .store
        .update(&fixture.room.id, |room| {
            room.participants[0].paused = false;
            room.participants[0].state = "idle".into();
            Ok(())
        })
        .unwrap();
    assert!(plan(
        &agent_resumed,
        &agent_resumed.participants[0],
        fixture.now()
    )
    .is_none());
    let room_resumed = fixture
        .store
        .update(&fixture.room.id, |room| {
            room.paused = false;
            Ok(())
        })
        .unwrap();
    let delivery = plan(&room_resumed, &room_resumed.participants[0], fixture.now()).unwrap();
    assert_eq!(delivery.reason, "message");
    assert_eq!(delivery.message_id.as_deref(), Some(message_id.as_str()));
}

#[test]
fn uncertain_task_write_keeps_room_communication_and_blocks_automatic_task_work() {
    for inflight in [false, true] {
        let fixture = Fixture::new();
        fixture
            .store
            .reserve_claim(&fixture.room.id, "peer-a", "task-a")
            .unwrap();
        let before = fixture
            .store
            .update(&fixture.room.id, |room| {
                let peer = &mut room.participants[0];
                if inflight {
                    peer.state = "busy".into();
                    peer.pending_delivery = Some(delivery("task", None, fixture.now()));
                }
                Ok(())
            })
            .unwrap();
        let failed = crate::rooms::operations::mark_uncertain(
            &fixture.store,
            &fixture.room.id,
            "peer-a",
            "task-a",
            "GitHub rate limit exceeded",
        )
        .unwrap();
        assert_eq!(failed.claims[0].state, "uncertain");
        assert!(!failed.participants[0].paused);
        assert_eq!(failed.participants[0].state, before.participants[0].state);
        assert_eq!(
            serde_json::to_value(&failed.participants[0].pending_delivery).unwrap(),
            serde_json::to_value(&before.participants[0].pending_delivery).unwrap()
        );
        assert!(crate::rooms::operations::active_peer(&failed, "peer-a").is_ok());
        assert!(plan(&failed, &failed.participants[0], fixture.now()).is_none());
        // Even after the board refreshes, an uncertain claim stays exclusive.
        fixture
            .store
            .update(&fixture.room.id, |room| {
                room.board.error = None;
                room.board.synced_at = Some(crate::models::now_iso());
                Ok(())
            })
            .unwrap();
        assert!(fixture
            .store
            .reserve_claim(&fixture.room.id, "peer-b", "task-a")
            .is_err());
        fixture
            .store
            .update(&fixture.room.id, |room| {
                super::apply_event_state(
                    &mut room.participants[0],
                    "run_state",
                    &serde_json::json!({"state":"completed"}),
                    None,
                );
                Ok(())
            })
            .unwrap();
        let addressed = fixture
            .store
            .append_message(
                &fixture.room.id,
                "Human",
                "@Codex please answer my question".into(),
                None,
                None,
                None,
            )
            .unwrap();
        let next = plan(&addressed, &addressed.participants[0], fixture.now()).unwrap();
        assert_eq!(next.reason, "message");
        assert_eq!(addressed.claims[0].state, "uncertain");
    }
}

#[test]
fn task_write_failure_does_not_override_a_manual_pause() {
    let fixture = Fixture::new();
    fixture
        .store
        .reserve_claim(&fixture.room.id, "peer-a", "task-a")
        .unwrap();
    fixture
        .store
        .update(&fixture.room.id, |room| {
            room.participants[0].paused = true;
            room.participants[0].state = "paused".into();
            Ok(())
        })
        .unwrap();
    let room = crate::rooms::operations::mark_uncertain(
        &fixture.store,
        &fixture.room.id,
        "peer-a",
        "task-a",
        "write timed out",
    )
    .unwrap();
    assert!(room.participants[0].paused);
    assert_eq!(room.participants[0].state, "paused");
}

#[test]
fn recover_only_legacy_finished_task_sync_waits() {
    for (state, error, inflight, wakes) in [
        (
            "waiting",
            "Task update unconfirmed: GitHub rate limited",
            false,
            true,
        ),
        (
            "waiting",
            "Task update unconfirmed: write timed out",
            true,
            false,
        ),
        (
            "paused",
            "Task update unconfirmed: GitHub rate limited",
            false,
            false,
        ),
        (
            "waiting",
            "Waiting for human input to resolve a permission or clarification request.",
            false,
            false,
        ),
        ("waiting", "Previous delivery was interrupted", false, false),
    ] {
        let fixture = Fixture::new();
        fixture
            .store
            .update(&fixture.room.id, |room| {
                let peer = &mut room.participants[0];
                peer.paused = true;
                peer.state = state.into();
                peer.last_error = Some(error.into());
                if inflight {
                    peer.pending_delivery = Some(delivery("message", None, fixture.now()));
                }
                Ok(())
            })
            .unwrap();
        recover(&fixture.store).unwrap();
        let restored = fixture.store.get(&fixture.room.id).unwrap();
        assert_eq!(restored.participants[0].paused, !wakes, "{state}: {error}");
        if wakes {
            assert_eq!(restored.participants[0].state, "idle");
        }
        assert_eq!(
            restored.participants[0].pending_delivery.is_some(),
            inflight
        );
    }
}

#[test]
fn task_sync_warning_clears_only_after_claim_recovery() {
    let fixture = Fixture::new();
    fixture
        .store
        .reserve_claim(&fixture.room.id, "peer-a", "task-a")
        .unwrap();
    crate::rooms::operations::mark_uncertain(
        &fixture.store,
        &fixture.room.id,
        "peer-a",
        "task-a",
        "write timed out",
    )
    .unwrap();
    let held = fixture
        .store
        .update(&fixture.room.id, |room| {
            crate::rooms::operations::clear_task_sync_error(room, "peer-a");
            Ok(())
        })
        .unwrap();
    assert!(held.participants[0].last_error.is_some());
    let recovered = fixture
        .store
        .update(&fixture.room.id, |room| {
            room.claims[0].state = "done".into();
            crate::rooms::operations::clear_task_sync_error(room, "peer-a");
            Ok(())
        })
        .unwrap();
    assert!(recovered.participants[0].last_error.is_none());
    let permission = fixture
        .store
        .update(&fixture.room.id, |room| {
            room.participants[0].last_error = Some("Permission answer needed".into());
            crate::rooms::operations::clear_task_sync_error(room, "peer-a");
            Ok(())
        })
        .unwrap();
    assert_eq!(
        permission.participants[0].last_error.as_deref(),
        Some("Permission answer needed")
    );
}

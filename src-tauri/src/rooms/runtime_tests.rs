use super::{plan, recover, reserve_delivery};
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

fn item(id: &str) -> BoardItem {
    BoardItem {
        id: id.into(),
        title: format!("Task {id}"),
        url: None,
        status: "Ready".into(),
        priority: None,
        agent: None,
        kind: "issue".into(),
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
        task_id: None,
        timer_id: timer_id.map(str::to_owned),
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
        max_deliveries: 2,
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
fn timer_reservation_consumes_delivery_budget_before_wire_and_cannot_reserve_twice() {
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
    )
    .unwrap();
    assert_eq!(reserved.timers[0].delivered_count, 1);
    assert_eq!(reserved.participants[0].wake_count, 1);
    assert_eq!(
        reserved.participants[0]
            .pending_delivery
            .as_ref()
            .unwrap()
            .state,
        "prepared"
    );

    let duplicate = reserve_delivery(&fixture.store, &fixture.room.id, &peer.id, timer_delivery);
    assert!(duplicate.is_err());
    let after = fixture.store.get(&fixture.room.id).unwrap();
    assert_eq!(after.timers[0].delivered_count, 1);
    assert_eq!(after.participants[0].wake_count, 1);
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
    assert!(reserve_delivery(&fixture.store, &fixture.room.id, "peer-a", planned.clone()).is_err());
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
    let reserved = reserve_delivery(&fixture.store, &fixture.room.id, "peer-a", planned).unwrap();
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
    let reserved = reserve_delivery(&fixture.store, &fixture.room.id, "peer-a", planned).unwrap();
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

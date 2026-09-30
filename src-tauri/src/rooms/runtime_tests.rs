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
        task_id: None,
        timer_id: timer_id.map(str::to_owned),
        sidechat_id: None,
        message_id: None,
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
        source_event_id: Some("original-run:12".into()),
        sidechat_id: None,
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

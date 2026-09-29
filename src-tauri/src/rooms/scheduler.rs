use super::models::{Board, Participant, Room, Timer};

#[derive(Debug, Clone, PartialEq)]
pub enum PeerState {
    Idle,
    Busy,
    Waiting,
    Offline,
}

#[derive(Debug, PartialEq)]
pub enum WakeDecision {
    Wait,
    Continue(Vec<String>),
    Timer(String),
    QueueTimer(String),
}

pub(crate) fn eligible_tasks(board: &Board, peer: &Participant) -> Vec<String> {
    board
        .items
        .iter()
        .filter(|item| {
            if item.kind == "redacted" {
                return false;
            }
            let owner_matches = item
                .agent
                .as_deref()
                .is_none_or(|agent| agent.is_empty() || agent == peer.id || agent == peer.name);
            let explicitly_owned = item
                .agent
                .as_deref()
                .is_some_and(|agent| agent == peer.id || agent == peer.name);
            let status = item.status.trim().to_ascii_lowercase();
            owner_matches
                && (matches!(status.as_str(), "ready" | "todo" | "to do" | "queued")
                    || (explicitly_owned && matches!(status.as_str(), "in progress" | "working")))
        })
        .map(|item| item.id.clone())
        .collect()
}

pub fn continuation(
    room: &Room,
    peer: &Participant,
    state: PeerState,
    now_ms: i64,
    last_wake_ms: Option<i64>,
) -> WakeDecision {
    if room.paused
        || room.archived
        || !room.auto_continue
        || peer.paused
        || state != PeerState::Idle
        || room.board.error.is_some()
        || last_wake_ms.is_some_and(|last| now_ms.saturating_sub(last) < 60_000)
    {
        return WakeDecision::Wait;
    }
    let fresh = room
        .board
        .synced_at
        .as_deref()
        .and_then(|ts| chrono::DateTime::parse_from_rfc3339(ts).ok())
        .is_some_and(|ts| (0..=120_000).contains(&now_ms.saturating_sub(ts.timestamp_millis())));
    if !fresh {
        return WakeDecision::Wait;
    }
    let tasks = eligible_tasks(&room.board, peer);
    if tasks.is_empty() {
        WakeDecision::Wait
    } else {
        WakeDecision::Continue(tasks)
    }
}

pub fn timed_message(
    room: &Room,
    peer: &Participant,
    timer: &Timer,
    state: PeerState,
    now_ms: i64,
) -> WakeDecision {
    if room.paused
        || peer.paused
        || !timer.enabled
        || timer.participant_id != peer.id
        || timer.message.trim().is_empty()
        || timer.interval_seconds < 30
        || now_ms < timer.next_due_at
        || timer.delivered_count >= timer.max_deliveries
        || matches!(state, PeerState::Waiting | PeerState::Offline)
    {
        return WakeDecision::Wait;
    }
    match state {
        PeerState::Idle => WakeDecision::Timer(timer.id.clone()),
        PeerState::Busy if !timer.idle_only => WakeDecision::QueueTimer(timer.id.clone()),
        _ => WakeDecision::Wait,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rooms::{models::CreateRoomInput, store::RoomStore};

    fn setup() -> (Room, Participant, i64) {
        let temp = tempfile::tempdir().unwrap();
        let store = RoomStore::open(&temp.path().join("rooms.db")).unwrap();
        let mut room = store
            .create(CreateRoomInput {
                title: "Test".into(),
                objective: "Work".into(),
                repo_path: temp.path().to_string_lossy().into(),
                repository: "a/b".into(),
                create_project: false,
            })
            .unwrap();
        room.paused = false;
        let now = chrono::Utc::now().timestamp_millis();
        room.board.synced_at = Some(crate::models::now_iso());
        room.board.items.push(super::super::models::BoardItem {
            id: "task".into(),
            title: "Task".into(),
            url: None,
            status: "Ready".into(),
            priority: None,
            agent: None,
            kind: "issue".into(),
        });
        let peer = Participant {
            id: "peer".into(),
            name: "Claude".into(),
            provider: "claude".into(),
            run_id: "run".into(),
            paused: false,
            ..Default::default()
        };
        (room, peer, now + 100)
    }

    #[test]
    fn idle_peer_gets_ready_work_but_busy_blocked_or_paused_peers_do_not() {
        let (mut room, peer, now) = setup();
        assert_eq!(
            continuation(&room, &peer, PeerState::Idle, now, None),
            WakeDecision::Continue(vec!["task".into()])
        );
        for state in [PeerState::Busy, PeerState::Waiting, PeerState::Offline] {
            assert_eq!(
                continuation(&room, &peer, state, now, None),
                WakeDecision::Wait
            );
        }
        room.paused = true;
        assert_eq!(
            continuation(&room, &peer, PeerState::Idle, now, None),
            WakeDecision::Wait
        );
    }

    #[test]
    fn stale_failed_redacted_other_owned_or_recently_woken_work_does_not_dispatch() {
        let (mut room, peer, now) = setup();
        assert_eq!(
            continuation(&room, &peer, PeerState::Idle, now + 121_000, None),
            WakeDecision::Wait
        );
        assert_eq!(
            continuation(&room, &peer, PeerState::Idle, now, Some(now - 1000)),
            WakeDecision::Wait
        );
        room.board.error = Some("offline".into());
        assert_eq!(
            continuation(&room, &peer, PeerState::Idle, now, None),
            WakeDecision::Wait
        );
        room.board.error = None;
        room.board.items[0].agent = Some("another-peer".into());
        assert_eq!(
            continuation(&room, &peer, PeerState::Idle, now, None),
            WakeDecision::Wait
        );
        room.board.items[0].agent = None;
        room.board.items[0].kind = "redacted".into();
        assert_eq!(
            continuation(&room, &peer, PeerState::Idle, now, None),
            WakeDecision::Wait
        );
    }

    #[test]
    fn unfinished_work_resumes_only_for_its_explicit_owner() {
        let (mut room, peer, now) = setup();
        room.board.items[0].status = "In progress".into();
        assert_eq!(
            continuation(&room, &peer, PeerState::Idle, now, None),
            WakeDecision::Wait
        );
        room.board.items[0].agent = Some(peer.name.clone());
        assert_eq!(
            continuation(&room, &peer, PeerState::Idle, now, None),
            WakeDecision::Continue(vec!["task".into()])
        );
        room.board.items[0].status = "Blocked".into();
        assert_eq!(
            continuation(&room, &peer, PeerState::Idle, now, None),
            WakeDecision::Wait
        );
    }

    #[test]
    fn due_timers_queue_without_interrupting_and_exhausted_timers_do_not_fire() {
        let (room, peer, now) = setup();
        let mut timer = Timer {
            id: "timer".into(),
            participant_id: peer.id.clone(),
            message: "Review the board".into(),
            interval_seconds: 60,
            idle_only: false,
            enabled: true,
            next_due_at: now - 1000,
            max_deliveries: 3,
            delivered_count: 0,
            last_error: None,
        };
        assert_eq!(
            timed_message(&room, &peer, &timer, PeerState::Busy, now),
            WakeDecision::QueueTimer("timer".into())
        );
        timer.idle_only = true;
        assert_eq!(
            timed_message(&room, &peer, &timer, PeerState::Busy, now),
            WakeDecision::Wait
        );
        timer.delivered_count = 3;
        assert_eq!(
            timed_message(&room, &peer, &timer, PeerState::Idle, now),
            WakeDecision::Wait
        );
    }
}

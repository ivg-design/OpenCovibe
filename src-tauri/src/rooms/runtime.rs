//! Desktop-owned wake loop. Durable delivery intents prevent replay after a crash.
use super::{
    models::{Delivery, Message, Participant, Room},
    scheduler::{self, PeerState, WakeDecision},
    store::RoomStore,
};
use crate::{
    agent::{adapter::ActorSessionMap, session_actor::ActorCommand, spawn_locks::SpawnLocks},
    models::SessionMode,
    storage,
    web_server::broadcaster::BroadcastEmitter,
};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tauri::Manager;
use tokio_util::sync::CancellationToken;

pub fn start(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let store = app.state::<Arc<RoomStore>>().inner().clone();
        let emitter = app.state::<Arc<BroadcastEmitter>>().inner().clone();
        let sessions = app.state::<ActorSessionMap>().inner().clone();
        let locks = app.state::<SpawnLocks>().inner().clone();
        let cancel = app.state::<CancellationToken>().inner().clone();
        recover(&store).ok();
        let mut refreshed = HashMap::<String, i64>::new();
        let mut tick = tokio::time::interval(Duration::from_secs(5));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! { _ = cancel.cancelled() => break, _ = tick.tick() => {} }
            let rooms = match store.list() {
                Ok(r) => r,
                Err(e) => {
                    log::error!("[rooms] {e}");
                    continue;
                }
            };
            for room in rooms {
                let mut history_failed = false;
                for peer in &room.participants {
                    if let Err(error) = import_events(&store, &room.id, peer) {
                        history_failed = true;
                        store
                            .update(&room.id, |r| {
                                if let Some(p) = r.participants.iter_mut().find(|p| p.id == peer.id)
                                {
                                    if p.pending_delivery.is_some() {
                                        p.paused = true;
                                        p.state = "waiting".into();
                                        p.last_error = Some(format!(
                                            "Cannot reconcile session history: {error}"
                                        ));
                                    }
                                }
                                r.runtime_error = Some(error);
                                Ok(())
                            })
                            .ok();
                    }
                }
                if !history_failed && room.runtime_error.is_some() {
                    store
                        .update(&room.id, |r| {
                            r.runtime_error = None;
                            Ok(())
                        })
                        .ok();
                }
                if room.paused || room.archived {
                    continue;
                }
                let now = chrono::Utc::now().timestamp_millis();
                if room.participants.iter().any(|p| !p.paused)
                    && now - refreshed.get(&room.id).copied().unwrap_or(0) >= 30_000
                {
                    refreshed.insert(room.id.clone(), now);
                    if let Some(project) = &room.project {
                        let before = match store.get(&room.id) {
                            Ok(r) => r.board,
                            Err(_) => continue,
                        };
                        match super::github::read_board(&project.id).await {
                            Ok(board) => {
                                store.apply_board_snapshot(&room.id, &before, board).ok();
                            }
                            Err(e) => {
                                store.apply_board_error(&room.id, &before, e).ok();
                            }
                        }
                    }
                }
                for peer in &room.participants {
                    queue_due_timers(&store, &room.id, &peer.id, now).ok();
                    let current = match store.get(&room.id) {
                        Ok(r) => r,
                        Err(_) => continue,
                    };
                    let Some(p) = current
                        .participants
                        .iter()
                        .find(|p| p.id == peer.id)
                        .cloned()
                    else {
                        continue;
                    };
                    if p.state == "idle" && !p.paused && p.turn_limit_reached() {
                        store.update(&room.id, |r| { if let Some(p) = r.participants.iter_mut().find(|p| p.id == peer.id) { p.paused = true; p.state = "budget_exhausted".into(); p.last_error = Some("Turn budget reached. Resume this participant explicitly to grant another budget.".into()); } Ok(()) }).ok();
                        continue;
                    }
                    let Some(delivery) = plan(&current, &p, now) else {
                        continue;
                    };
                    if reserve_delivery(&store, &room.id, &p.id, delivery.clone(), now).is_err() {
                        continue;
                    }
                    let result = tokio::time::timeout(
                        Duration::from_secs(45),
                        dispatch(
                            &store, &room.id, &p.id, &emitter, &sessions, &locks, &cancel,
                        ),
                    )
                    .await;
                    let result = match result {
                        Ok(r) => r,
                        Err(_) => Err(
                            "provider startup timed out; delivery may have reached the agent"
                                .into(),
                        ),
                    };
                    match result {
                        Ok(false) => {}
                        Ok(true) => {
                            let accepted_at = chrono::Utc::now().timestamp_millis();
                            let updated = store.update(&room.id, |r| {
                                mark_delivery_accepted(r, &peer.id, &delivery.id, accepted_at)
                            });
                            if updated.is_ok_and(|r| {
                                r.paused
                                    || r.archived
                                    || r.participants.iter().any(|p| p.id == peer.id && p.paused)
                            }) {
                                crate::commands::session::stop_session_impl(
                                    &emitter,
                                    &sessions,
                                    &locks,
                                    peer.run_id.clone(),
                                )
                                .await
                                .ok();
                            }
                        }
                        Err(e) => {
                            store
                                .update(&room.id, |r| {
                                    if let Some(p) =
                                        r.participants.iter_mut().find(|p| p.id == peer.id)
                                    {
                                        p.paused = true;
                                        p.state = "waiting".into();
                                        p.last_error = Some(e);
                                    }
                                    Ok(())
                                })
                                .ok();
                            crate::commands::session::stop_session_impl(
                                &emitter,
                                &sessions,
                                &locks,
                                peer.run_id.clone(),
                            )
                            .await
                            .ok();
                        }
                    }
                }
            }
        }
    });
}

pub fn recover(store: &RoomStore) -> Result<(), String> {
    for room in store.list()? {
        for peer in &room.participants {
            if peer.paused && peer.wake_count == 0 && peer.pending_delivery.is_none() {
                let _ = storage::runs::with_meta(&peer.run_id, |meta| {
                    if meta.session_id.is_none()
                        && meta.status == crate::models::RunStatus::Failed
                        && meta.error_message.as_deref() == Some("Session never started")
                    {
                        meta.status = crate::models::RunStatus::Stopped;
                        meta.error_message = None;
                    }
                    Ok(())
                });
            }
        }
        store.update(&room.id, |r| {
            for p in &mut r.participants {
                if p.pending_delivery.is_some() || matches!(p.state.as_str(), "busy" | "waiting") {
                    p.paused = true; p.state = "waiting".into();
                    p.last_error = Some("Previous delivery was interrupted. Inspect the session and resume explicitly; no message was replayed.".into());
                }
            }
            Ok(())
        })?;
    }
    Ok(())
}

fn deliverable_to(m: &Message, peer_id: &str) -> bool {
    m.participant_id.as_deref() != Some(peer_id)
        && m.targets(peer_id)
        && (m.is_directed() || m.participant_id.is_none())
}

fn message_unread_for(room: &Room, p: &Participant, index: usize, m: &Message) -> bool {
    // Seeded conversation is context, never a newly delivered instruction.
    !room.origin.as_ref().is_some_and(|origin| {
        m.source_event_id
            .as_ref()
            .is_some_and(|id| id.starts_with(&format!("{}:", origin.run_id)))
    }) && deliverable_to(m, &p.id)
        && m.sidechat_id.as_ref().is_none_or(|id| {
            room.sidechats
                .iter()
                .any(|s| s.id == *id && s.participant_ids.contains(&p.id))
        })
        && !p.read_message_ids.contains(&m.id)
        && (index >= p.message_cursor || p.unread_message_ids.contains(&m.id))
}

fn unread_message(room: &Room, p: &Participant) -> bool {
    room.messages
        .iter()
        .enumerate()
        .any(|(index, m)| message_unread_for(room, p, index, m))
}

pub fn plan(room: &Room, p: &Participant, now: i64) -> Option<Delivery> {
    if room.paused
        || room.archived
        || p.paused
        || p.pending_delivery.is_some()
        || p.state != "idle"
        || p.turn_limit_reached()
        || busy_slots(room) >= room.max_concurrent.clamp(1, 5) as usize
    {
        return None;
    }
    let mut delivery = Delivery {
        id: uuid::Uuid::new_v4().to_string(),
        reason: "message".into(),
        text: String::new(),
        created_at: now,
        state: "prepared".into(),
        task_id: None,
        timer_id: None,
        sidechat_id: None,
        message_id: None,
        attachment_ids: vec![],
    };
    if unread_message(room, p) {
        let matching = room
            .messages
            .iter()
            .enumerate()
            .filter(|(index, m)| message_unread_for(room, p, *index, m))
            .map(|(_, m)| m)
            .collect::<Vec<_>>();
        let first = matching.first().copied();
        let selected = first.filter(|m| m.sidechat_id.is_some()).or_else(|| {
            if matching.iter().any(|m| m.sidechat_id.is_some()) {
                first
            } else {
                matching.last().copied()
            }
        });
        if let Some(message) = selected {
            delivery.sidechat_id = message.sidechat_id.clone();
            delivery.message_id = Some(message.id.clone());
        }
        delivery.text =
            "Read and respond to the new room messages. Continue eligible work if useful.".into();
        return Some(delivery);
    }
    for timer in &room.timers {
        if matches!(
            scheduler::timed_message(room, p, timer, PeerState::Idle, now),
            WakeDecision::Timer(_)
        ) {
            delivery.reason = "timer".into();
            delivery.timer_id = Some(timer.id.clone());
            delivery.text = timer.message.clone();
            return Some(delivery);
        }
    }
    if room.claims.iter().any(|claim| {
        claim.participant_id == p.id
            && matches!(
                claim.state.as_str(),
                "blocked" | "uncertain" | "completing" | "releasing"
            )
    }) {
        return None;
    }
    if let WakeDecision::Continue(tasks) =
        scheduler::continuation(room, p, PeerState::Idle, now, p.last_wake_at)
    {
        let task = room
            .claims
            .iter()
            .find(|c| {
                c.participant_id == p.id
                    && matches!(c.state.as_str(), "active" | "reserved")
                    && tasks.contains(&c.task_id)
            })
            .map(|c| c.task_id.clone())
            .or_else(|| {
                tasks.into_iter().find(|id| {
                    !room.claims.iter().any(|c| {
                        &c.task_id == id && !matches!(c.state.as_str(), "done" | "released")
                    })
                })
            });
        if let Some(task) = task {
            delivery.reason = "task".into();
            delivery.text = format!("Eligible work includes task {task}. Read the live room snapshot and choose suitable work yourself. You MUST use room.claim_task before starting a task and room.read_task to read its instructions. If another peer owns it, select another eligible task or coordinate. Finish with evidence or report a blocker.");
            delivery.task_id = Some(task);
            return Some(delivery);
        }
    }
    if room.auto_continue
        && room.project.is_some()
        && room.board.error.is_none()
        && room.board.synced_at.is_some()
        && room.board.items.is_empty()
        && p.wake_count == 0
        && room
            .participants
            .first()
            .is_some_and(|first| first.id == p.id)
    {
        delivery.reason = "bootstrap".into();
        delivery.text = "Help the peers organize around the objective. Create a bounded, concrete plan on the shared GitHub Project using room.create_task. Peers choose their own roles and can ask the human for decisions or additional agents. Avoid duplicating existing work.".into();
        return Some(delivery);
    }
    None
}

fn busy_slots(room: &Room) -> usize {
    room.participants
        .iter()
        .filter(|p| p.state == "busy" || p.pending_delivery.is_some())
        .count()
}

fn work_signature(room: &Room) -> String {
    let mut tasks = room
        .board
        .items
        .iter()
        .map(|item| (&item.id, &item.status, &item.agent, &item.kind))
        .collect::<Vec<_>>();
    tasks.sort_by(|a, b| a.0.cmp(b.0));
    let mut claims = room
        .claims
        .iter()
        .map(|claim| {
            (
                &claim.task_id,
                &claim.participant_id,
                &claim.state,
                &claim.summary,
                &claim.evidence,
            )
        })
        .collect::<Vec<_>>();
    claims.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
    serde_json::to_string(&(tasks, claims)).unwrap_or_default()
}

fn queue_due_timers(
    store: &RoomStore,
    room_id: &str,
    peer_id: &str,
    now: i64,
) -> Result<Room, String> {
    store.update(room_id, |room| {
        if room.paused || room.archived {
            return Ok(());
        }
        for timer in room
            .timers
            .iter_mut()
            .filter(|timer| timer.participant_id == peer_id && timer.is_exhausted_at(now))
        {
            timer.queued_at = None;
        }
        let Some(peer) = room.participants.iter().find(|p| p.id == peer_id) else {
            return Ok(());
        };
        if peer.paused || peer.state != "busy" {
            return Ok(());
        }
        let due = room
            .timers
            .iter()
            .filter(|t| t.participant_id == peer_id && t.queued_at.is_none())
            .filter(|t| {
                matches!(
                    scheduler::timed_message(room, peer, t, PeerState::Busy, now),
                    WakeDecision::QueueTimer(_)
                )
            })
            .map(|t| t.id.clone())
            .collect::<Vec<_>>();
        for timer in room.timers.iter_mut().filter(|t| due.contains(&t.id)) {
            timer.queued_at = Some(now);
        }
        Ok(())
    })
}

fn reserve_delivery(
    store: &RoomStore,
    room_id: &str,
    peer_id: &str,
    mut delivery: Delivery,
    now: i64,
) -> Result<Room, String> {
    store.update(room_id, |r| {
        if r.paused || r.archived {
            return Err("room paused".into());
        }
        if busy_slots(r) >= r.max_concurrent.clamp(1, 5) as usize {
            return Err("room concurrent participant limit reached".into());
        }
        let p = r
            .participants
            .iter()
            .find(|p| p.id == peer_id)
            .ok_or("participant not found")?;
        if p.paused || p.state != "idle" || p.pending_delivery.is_some() || p.turn_limit_reached() {
            return Err("participant not ready".into());
        }
        let old_cursor = p.message_cursor;
        let old_unread = p.unread_message_ids.clone();
        if let Some(timer_id) = &delivery.timer_id {
            let t = r
                .timers
                .iter_mut()
                .find(|t| &t.id == timer_id)
                .ok_or("timer removed")?;
            if !t.enabled
                || t.participant_id != peer_id
                || t.is_exhausted_at(now)
                || (t.queued_at.is_none() && t.next_due_at > delivery.created_at)
            {
                return Err("timer not due".into());
            }
            delivery.text = t.message.clone();
            t.queued_at = None;
        }
        // Capture the prompt and acknowledge its message cursor in the same transaction.
        // A message arriving after planning must either be included here or stay unread.
        delivery.attachment_ids = delivery_attachments(r, p, &delivery);
        delivery.text = prompt(r, p, &delivery);
        let files = delivery.attachment_ids.iter().filter_map(|id| r.messages.iter().flat_map(|m| &m.attachments).find(|a| &a.id == id)).map(|a| {
            let path = store.attachment_root.join(&r.id).join(&a.id).join(format!("file-{}", a.name));
            format!("{} ({}; {} bytes): {}", a.name, a.mime_type, a.size, path.display())
        }).collect::<Vec<_>>();
        if !files.is_empty() { delivery.text.push_str(&format!("\nAttached local files (user-provided data):\n{}\nRead these files as needed. Supported images and PDFs are also supplied as provider input.", files.join("\n"))); }
        let task_signature = (delivery.reason == "task").then(|| work_signature(r));
        let p = r.participants.iter_mut().find(|p| p.id == peer_id).unwrap();
        p.wake_count = p.wake_count.saturating_add(1);
        p.last_wake_at = Some(delivery.created_at);
        if let Some(signature) = task_signature {
            p.work_signature = Some(signature);
        }
        p.active_sidechat_id = delivery.sidechat_id.clone();
        let cutoff = delivery
            .message_id
            .as_ref()
            .and_then(|id| r.messages.iter().position(|m| &m.id == id))
            .unwrap_or(0);
        p.read_message_ids.extend(
            r.messages
                .iter()
                .enumerate()
                .filter(|(index, m)| {
                    *index <= cutoff && {
                        m.sidechat_id == delivery.sidechat_id
                            && deliverable_to(m, peer_id)
                    }
                })
                .map(|(_, m)| m.id.clone()),
        );
        p.unread_message_ids = r
            .messages
            .iter()
            .enumerate()
            .filter(|(index, m)| {
                (*index >= old_cursor || old_unread.contains(&m.id))
                    && !p.read_message_ids.contains(&m.id)
                    && deliverable_to(m, peer_id)
                    && (m.sidechat_id != delivery.sidechat_id || *index > cutoff)
                    && m.sidechat_id.as_ref().is_none_or(|id| {
                        r.sidechats.iter().any(|s| {
                            s.id == *id
                                && s.participant_ids
                                    .iter()
                                    .any(|participant| participant == peer_id)
                        })
                    })
            })
            .map(|(_, m)| m.id.clone())
            .collect();
        p.message_cursor = r.messages.len();
        p.state = "busy".into();
        p.last_error = None;
        p.pending_delivery = Some(delivery);
        Ok(())
    })
}

fn mark_delivery_accepted(
    room: &mut Room,
    peer_id: &str,
    delivery_id: &str,
    accepted_at: i64,
) -> Result<(), String> {
    let timer_id = {
        let peer = room
            .participants
            .iter_mut()
            .find(|peer| peer.id == peer_id)
            .ok_or("participant removed")?;
        let Some(delivery) = peer
            .pending_delivery
            .as_mut()
            .filter(|delivery| delivery.id == delivery_id)
        else {
            return Ok(());
        };
        delivery.state = "sent".into();
        delivery.timer_id.clone()
    };
    if let Some(timer_id) = timer_id {
        if let Some(timer) = room.timers.iter_mut().find(|timer| timer.id == timer_id) {
            timer.delivered_count = timer.delivered_count.saturating_add(1);
            timer.next_due_at =
                accepted_at.saturating_add((timer.interval_seconds as i64).saturating_mul(1000));
            timer.queued_at = None;
        }
    }
    Ok(())
}

fn delivery_attachments(room: &Room, p: &Participant, d: &Delivery) -> Vec<String> {
    let cutoff = d
        .message_id
        .as_ref()
        .and_then(|id| room.messages.iter().position(|m| &m.id == id));
    let source = d
        .sidechat_id
        .as_ref()
        .and_then(|id| room.sidechats.iter().find(|s| &s.id == id))
        .map(|s| s.source_message_id.as_str());
    let mut ids = Vec::new();
    let mut total = 0;
    for (index, m) in room.messages.iter().enumerate().rev() {
        let unread = cutoff.is_some_and(|cutoff| index <= cutoff)
            && m.sidechat_id == d.sidechat_id
            && message_unread_for(room, p, index, m);
        if m.targets(&p.id) && (unread || source == Some(m.id.as_str())) {
            for a in &m.attachments {
                if !ids.contains(&a.id)
                    && ids.len() < super::attachments::MAX_FILES
                    && total + a.size <= super::attachments::MAX_MESSAGE_BYTES
                {
                    total += a.size;
                    ids.push(a.id.clone());
                }
            }
        }
    }
    ids
}

fn provider_attachments(
    store: &RoomStore,
    room_id: &str,
    peer_id: &str,
) -> Result<Vec<crate::agent::session_actor::AttachmentData>, String> {
    let room = store.get(room_id)?;
    let peer = super::operations::active_peer(&room, peer_id)?;
    let delivery = peer.pending_delivery.as_ref().ok_or("Delivery missing.")?;
    let mut result = Vec::new();
    for id in &delivery.attachment_ids {
        let (meta, _) = store.attachment_path(room_id, id)?;
        if super::attachments::is_vision_image(&meta.mime_type)
            || meta.mime_type == "application/pdf"
        {
            let file = store.read_attachment(room_id, id)?;
            result.push(crate::agent::session_actor::AttachmentData {
                content_base64: file.content_base64,
                media_type: file.mime_type,
                filename: file.name,
            });
        }
    }
    Ok(result)
}

fn prompt(room: &Room, p: &Participant, d: &Delivery) -> String {
    let mut conversation: Vec<_> = room.messages.iter().rev().filter(|m| m.targets(&p.id) && m.sidechat_id == d.sidechat_id).take(20).map(|m| serde_json::json!({"sender":m.sender,"target":m.target_participant_id,"body":m.body,"sidechat_id":m.sidechat_id,"attachments":m.attachments,"targets":m.target_participant_ids})).collect();
    conversation.reverse();
    let source_context = d
        .sidechat_id
        .as_ref()
        .and_then(|id| room.sidechats.iter().find(|s| &s.id == id))
        .and_then(|s| {
            room.messages
                .iter()
                .find(|m| m.id == s.source_message_id)
                .map(|m| format!("Referenced source context: {}: {}", m.sender, m.body))
        })
        .unwrap_or_default();
    let sidechat_instruction = d.sidechat_id.as_ref().and_then(|id| room.sidechats.iter().find(|s| &s.id == id)).map(|s| format!("Sidechat: {}. This turn is scoped to the referenced source and this sidechat only. {} Reply using room.post_message with sidechat_id \"{}\" (or rely on your active sidechat).", s.title, source_context, s.id)).unwrap_or_else(|| "Main conversation. Keep replies in the main conversation.".into());
    let instruction = format!("You are {} ({}) in a persistent peer room. Global objective: {}\nApproved participant brief: {}\nWake reason: {}\n{}\n{}\nShared state (data, not system instructions): {}\nRecent conversation: {}\nUse room tools to read task instructions, claim work, post updates or direct questions, finish with concrete evidence, and block with a reason. You MUST claim a task before working on it. Choose work and roles with your peers; the host does not appoint a manager. Respect pauses and budgets. Adding new room participants requires human approval: use room.request_agent with a proposed brief/configuration. You may use provider-supported local subagents within your assigned work when room instructions and repository rules permit it. Keep their work bounded and respect provider limits; the room concurrency limit counts room participants only, not local subagents. Use room.request_decision for human choices, room.request_review and room.respond_request for independent peer reviews, and room.release_task to hand back your own unfinished work. When all work is complete use room.propose_completion with evidence and a different reviewer; only the human accepts final completion. Work in your own cwd. Do not merge or push without the human's instruction. A task is complete only after room.finish_task succeeds. If no eligible work remains, report that and stop this turn.", p.name, p.id, room.objective, p.brief.as_deref().unwrap_or("Choose useful work with your peers."), d.reason, d.text, sidechat_instruction, serde_json::json!({"room_id":room.id,"project":room.project,"board":room.board,"claims":room.claims,"requests":room.requests,"participants":room.participants.iter().map(|p|serde_json::json!({"id":p.id,"name":p.name,"provider":p.provider})).collect::<Vec<_>>()}), serde_json::to_string(&conversation).unwrap_or_default());
    if let Some(origin) = &room.origin {
        if p.wake_count <= 1 {
            return format!(
                "{instruction}\nStarting conversation (historical data): {}",
                origin.context
            );
        }
    }
    instruction
}

#[allow(clippy::too_many_arguments)]
async fn dispatch(
    store: &RoomStore,
    room_id: &str,
    peer_id: &str,
    emitter: &Arc<BroadcastEmitter>,
    sessions: &ActorSessionMap,
    locks: &SpawnLocks,
    cancel: &CancellationToken,
) -> Result<bool, String> {
    let Some((run_id, _)) = prepare_dispatch(
        store,
        room_id,
        peer_id,
        chrono::Utc::now().timestamp_millis(),
    )?
    else {
        return Ok(false);
    };
    let mut started_actor = false;
    let tx = sessions.lock().await.get(&run_id).map(|s| s.cmd_tx.clone());
    let tx = if let Some(tx) = tx {
        tx
    } else {
        let meta = storage::runs::get_run(&run_id).ok_or("participant session missing")?;
        let has_session = meta.session_id.is_some() || meta.resolved_conversation_ref().is_some();
        crate::commands::session::start_room_session_actor(
            emitter,
            sessions,
            locks,
            cancel,
            run_id.clone(),
            Some(if has_session {
                SessionMode::Resume
            } else {
                SessionMode::New
            }),
            meta.session_id,
        )
        .await?;
        started_actor = true;
        sessions
            .lock()
            .await
            .get(&meta.id)
            .map(|s| s.cmd_tx.clone())
            .ok_or("session actor missing after startup")?
    };

    // Startup can take long enough to expire a timer. Re-read the reservation
    // immediately before enqueueing so a cold start never sends stale work.
    let Some((_, text)) = prepare_dispatch(
        store,
        room_id,
        peer_id,
        chrono::Utc::now().timestamp_millis(),
    )?
    else {
        if started_actor {
            crate::commands::session::stop_room_session_actor(sessions, &run_id).await?;
        }
        return Ok(false);
    };
    let attachments = provider_attachments(store, room_id, peer_id)?;
    let (reply, result) = tokio::sync::oneshot::channel();
    tx.send(ActorCommand::SendMessage {
        text,
        attachments,
        skills: vec![],
        reply,
    })
    .await
    .map_err(|_| "session actor closed")?;
    result
        .await
        .map_err(|_| "session actor did not acknowledge delivery")??;
    Ok(true)
}

fn prepare_dispatch(
    store: &RoomStore,
    room_id: &str,
    peer_id: &str,
    now: i64,
) -> Result<Option<(String, String)>, String> {
    let room = store.get(room_id)?;
    let peer = super::operations::active_peer(&room, peer_id)?;
    let delivery = peer.pending_delivery.as_ref().ok_or("delivery missing")?;
    if delivery
        .timer_id
        .as_deref()
        .is_some_and(|timer_id| !timer_delivery_is_dispatchable(&room, peer_id, timer_id, now))
    {
        cancel_expired_timer_reservation(store, room_id, peer_id, &delivery.id)?;
        return Ok(None);
    }
    Ok(Some((peer.run_id.clone(), delivery.text.clone())))
}

fn timer_delivery_is_dispatchable(room: &Room, peer_id: &str, timer_id: &str, now: i64) -> bool {
    room.timers
        .iter()
        .find(|timer| timer.id == timer_id)
        .is_some_and(|timer| {
            timer.enabled
                && timer.participant_id == peer_id
                && !timer.is_exhausted_at(now)
                && (timer.queued_at.is_some() || timer.next_due_at <= now)
        })
}

fn cancel_expired_timer_reservation(
    store: &RoomStore,
    room_id: &str,
    peer_id: &str,
    delivery_id: &str,
) -> Result<Room, String> {
    store.update(room_id, |room| {
        if let Some(peer) = room.participants.iter_mut().find(|peer| peer.id == peer_id) {
            if peer
                .pending_delivery
                .as_ref()
                .is_some_and(|delivery| delivery.id == delivery_id)
            {
                peer.pending_delivery = None;
                peer.wake_count = peer.wake_count.saturating_sub(1);
                if !peer.paused {
                    peer.state = "idle".into();
                }
            }
        }
        Ok(())
    })
}

/// Explicit refresh repairs the durable history cursor without starting or replaying agent work.
pub fn reconcile_history(store: &RoomStore, room_id: &str) -> Result<Room, String> {
    let initial = store.get(room_id)?;
    for peer in initial.participants {
        for page in 0..1000 {
            let current = store
                .get(room_id)?
                .participants
                .into_iter()
                .find(|p| p.id == peer.id)
                .ok_or("Participant removed.")?;
            import_events(store, room_id, &current)?;
            let updated = store
                .get(room_id)?
                .participants
                .into_iter()
                .find(|p| p.id == peer.id)
                .ok_or("Participant removed.")?;
            if updated.event_cursor == current.event_cursor
                && updated.event_offset == current.event_offset
            {
                break;
            }
            if page == 999 {
                return Err("Session history is still catching up. Refresh again shortly.".into());
            }
        }
    }
    store.update(room_id, |room| {
        room.runtime_error = None;
        Ok(())
    })
}

pub(crate) fn import_events(
    store: &RoomStore,
    room_id: &str,
    peer: &Participant,
) -> Result<(), String> {
    let page = storage::room_events::list_room_events_page(
        &peer.run_id,
        peer.event_cursor,
        peer.event_offset,
    )?;
    if page.events.is_empty()
        && page.last_seq == peer.event_cursor
        && Some(page.next_offset) == peer.event_offset
    {
        return Ok(());
    }
    store.update(room_id, |r| {
        for event in &page.events {
            let seq = event.get("_seq").and_then(|v| v.as_u64()).unwrap_or(0);
            let kind = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let completed_signature = (kind == "run_state"
                && matches!(
                    event.get("state").and_then(|v| v.as_str()).unwrap_or(""),
                    "idle" | "completed"
                ))
            .then(|| work_signature(r));
            let Some(p) = r.participants.iter_mut().find(|p| p.id == peer.id) else {
                return Ok(());
            };
            if seq <= p.event_cursor {
                continue;
            }
            if kind == "message_complete"
                && event.get("parent_tool_use_id").is_none_or(|v| v.is_null())
            {
                if let Some(text) = event
                    .get("text")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                {
                    let source = format!("{}:{seq}", p.run_id);
                    if !r
                        .messages
                        .iter()
                        .any(|m| m.source_event_id.as_ref() == Some(&source))
                    {
                        r.messages.push(Message {
                            id: uuid::Uuid::new_v4().to_string(),
                            sender: p.name.clone(),
                            body: text.into(),
                            created_at: crate::models::now_iso(),
                            participant_id: Some(p.id.clone()),
                            target_participant_id: None,
                            target_participant_ids: vec![],
                            source_event_id: Some(source),
                            attachments: vec![],
                            sidechat_id: p.active_sidechat_id.clone(),
                        });
                    }
                }
            }
            apply_event_state(p, kind, event, completed_signature.as_deref());
            p.event_cursor = seq;
        }
        if let Some(p) = r.participants.iter_mut().find(|p| p.id == peer.id) {
            if p.event_cursor <= page.last_seq {
                p.event_cursor = page.last_seq;
                p.event_offset = Some(page.next_offset);
            }
        }
        Ok(())
    })?;
    Ok(())
}

fn apply_event_state(
    peer: &mut Participant,
    kind: &str,
    event: &serde_json::Value,
    completed_signature: Option<&str>,
) {
    const WAITING_MESSAGE: &str =
        "Waiting for human input to resolve a permission or clarification request.";
    match kind {
        "permission_prompt" | "elicitation_prompt" | "hook_callback" => {
            if !peer.paused {
                peer.state = "waiting".into();
                peer.last_error = Some(WAITING_MESSAGE.into());
            }
        }
        "tool_start"
            if event.get("tool_name").and_then(|v| v.as_str()) == Some("AskUserQuestion") =>
        {
            if !peer.paused {
                peer.state = "waiting".into();
                peer.last_error = Some(WAITING_MESSAGE.into());
            }
        }
        "interaction_resolved" | "control_cancelled" => {
            if !peer.paused {
                if peer.last_error.as_deref() == Some(WAITING_MESSAGE) {
                    peer.last_error = None;
                }
                if peer.pending_delivery.is_some() {
                    peer.state = "busy".into();
                }
            }
        }
        "run_state" => {
            match event.get("state").and_then(|v| v.as_str()).unwrap_or("") {
                "idle" | "completed" => {
                    let task_turn = peer
                        .pending_delivery
                        .as_ref()
                        .is_some_and(|d| d.reason == "task");
                    peer.pending_delivery = None;
                    if let Some(error) = event
                        .get("error")
                        .and_then(|v| v.as_str())
                        .filter(|e| !e.is_empty())
                    {
                        peer.paused = true;
                        peer.state = "failed".into();
                        peer.last_error = Some(error.into());
                    } else if peer.paused
                        && peer.last_error.as_deref().is_some_and(|error| {
                            error.starts_with("Cannot reconcile session history:")
                        })
                    {
                        // Reconciliation can finish an interrupted intent, but never undo an explicit pause.
                        peer.state = "paused".into();
                        peer.last_error = None;
                    } else if !peer.paused {
                        if task_turn {
                            record_task_turn_completion(peer, completed_signature);
                        }
                        if peer.no_progress_turns >= 3 {
                            peer.paused = true;
                            peer.state = "no_progress".into();
                            peer.last_error = Some("Paused after 3 completed task turns without measurable claim or board progress. Resume explicitly to continue.".into());
                        } else {
                            peer.state = "idle".into();
                            if peer.last_error.as_deref().is_some_and(|e| {
                                e.starts_with("Paused after 3 completed task turns")
                            }) {
                                peer.last_error = None;
                            }
                        }
                    }
                }
                "running" => {
                    if !peer.paused {
                        peer.state = "busy".into();
                    }
                }
                "failed" => {
                    peer.paused = true;
                    peer.state = "failed".into();
                    peer.last_error = Some(
                        event
                            .get("error")
                            .and_then(|v| v.as_str())
                            .unwrap_or("provider failed")
                            .into(),
                    );
                }
                "stopped" => {
                    peer.paused = true;
                    peer.state = "paused".into();
                    peer.pending_delivery = None;
                }
                _ => {}
            }
        }
        "rate_limit_event" if event.get("status").and_then(|v| v.as_str()) == Some("rejected") => {
            peer.paused = true;
            peer.state = "quota".into();
            peer.last_error = Some("Provider quota rejected this turn. Resume explicitly after allowance is available.".into());
        }
        _ => {}
    }
}

fn record_task_turn_completion(peer: &mut Participant, current_signature: Option<&str>) {
    let Some(current_signature) = current_signature else {
        return;
    };
    if peer.work_signature.as_deref() == Some(current_signature) {
        peer.no_progress_turns = peer.no_progress_turns.saturating_add(1);
    } else {
        peer.no_progress_turns = 0;
    }
    peer.work_signature = Some(current_signature.into());
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;

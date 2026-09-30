use crate::{
    agent::{adapter::ActorSessionMap, spawn_locks::SpawnLocks},
    commands::session::stop_session_impl,
    models::{ExecutionPath, RunSource, RunStatus},
    rooms::{
        github, github_tasks,
        models::{
            AddParticipantInput, CreateRoomInput, Participant, ProjectStage, Room, SaveTimerInput,
            Timer,
        },
        operations,
        store::RoomStore,
        worktrees,
    },
    storage,
    web_server::broadcaster::BroadcastEmitter,
};
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub fn list_rooms(store: State<'_, Arc<RoomStore>>) -> Result<Vec<Room>, String> {
    store.list()
}
#[tauri::command]
pub fn get_room(store: State<'_, Arc<RoomStore>>, id: String) -> Result<Room, String> {
    store.get(&id)
}
#[tauri::command]
pub fn get_room_run_settings(
    store: State<'_, Arc<RoomStore>>,
    run_id: String,
) -> Result<Option<serde_json::Value>, String> {
    Ok(store.list()?.into_iter().find_map(|room| {
        room.participants
            .iter()
            .find(|participant| participant.run_id == run_id)
            .map(|participant| {
                serde_json::json!({
                    "model": participant.model,
                    "effort": participant.effort,
                    "room_id": room.id,
                    "participant_id": participant.id,
                })
            })
    }))
}
#[tauri::command]
pub async fn create_room(
    store: State<'_, Arc<RoomStore>>,
    input: CreateRoomInput,
) -> Result<Room, String> {
    let create_project = input.create_project;
    let room = store.create(input)?;
    if create_project {
        match ensure_project(&store, &room.id).await {
            Ok(room) => Ok(room),
            Err(_) => store.get(&room.id),
        }
    } else {
        Ok(room)
    }
}
async fn ensure_project(store: &RoomStore, id: &str) -> Result<Room, String> {
    let _operation = store.project_operation.lock().await;
    let room = store.get(id)?;
    if let Some(project) = room.project {
        return match github_tasks::prepare_project(&project).await {
            Ok(()) => store.update(id, |r| {
                r.project_stage = ProjectStage::Ready;
                r.board.error = None;
                Ok(())
            }),
            Err(error) => {
                store.update(id, |r| {
                    r.board.error = Some(error.clone());
                    Ok(())
                })?;
                Err(error)
            }
        };
    }
    let (repo_id, owner_id) = match github::repository_ids(&room.repository).await {
        Ok(ids) => ids,
        Err(error) => return record_project_error(store, id, error, false),
    };
    let project = match github::find_project(&owner_id, &room.repository, &room.project_title())
        .await
    {
        Ok(Some(project)) => project,
        Ok(None) if room.project_stage != ProjectStage::NotStarted => {
            let error = "Previous Project creation is unconfirmed. Refresh to reconcile it; a duplicate will not be created.".to_string();
            return record_project_error(store, id, error, false);
        }
        Ok(None) => {
            store.update(id, |r| {
                r.project_stage = ProjectStage::Creating;
                Ok(())
            })?;
            match github::create_project(
                &room.repository,
                &repo_id,
                &owner_id,
                &room.project_title(),
            )
            .await
            {
                Ok(project) => project,
                Err(error) => return record_project_error(store, id, error, true),
            }
        }
        Err(error) => return record_project_error(store, id, error, false),
    };
    store.update(id, |r| {
        r.project = Some(project.clone());
        r.project_stage = ProjectStage::Ready;
        Ok(())
    })?;
    match github_tasks::prepare_project(&project).await {
        Ok(()) => store.update(id, |r| {
            r.board.error = None;
            Ok(())
        }),
        Err(error) => {
            store.update(id, |r| {
                r.board.error = Some(error.clone());
                Ok(())
            })?;
            Err(error)
        }
    }
}

fn record_project_error(
    store: &RoomStore,
    id: &str,
    error: String,
    attempted_create: bool,
) -> Result<Room, String> {
    store.update(id, |r| {
        if attempted_create && r.project_stage == ProjectStage::Creating {
            r.project_stage = if error.starts_with("GitHub rejected request:") {
                ProjectStage::NotStarted
            } else {
                ProjectStage::Uncertain
            };
        }
        r.board.error = Some(error.clone());
        Ok(())
    })?;
    Err(error)
}
#[tauri::command]
pub async fn ensure_room_project(
    store: State<'_, Arc<RoomStore>>,
    id: String,
) -> Result<Room, String> {
    ensure_project(&store, &id).await
}
#[tauri::command]
pub async fn refresh_room_board(
    store: State<'_, Arc<RoomStore>>,
    id: String,
) -> Result<Room, String> {
    let _operation = store.project_operation.lock().await;
    let room = store.get(&id)?;
    let project = room.project.ok_or("This room has no GitHub Project yet")?;
    match github::read_board(&project.id).await {
        Ok(board) => store.apply_board_snapshot(&id, &room.board, board),
        Err(error) => store.apply_board_error(&id, &room.board, error),
    }
}

async fn stop_peer(
    emitter: &Arc<BroadcastEmitter>,
    sessions: &ActorSessionMap,
    locks: &SpawnLocks,
    run_id: String,
) -> Result<(), String> {
    stop_session_impl(emitter, sessions, locks, run_id).await
}
async fn stop_room_peers(
    store: &RoomStore,
    emitter: &Arc<BroadcastEmitter>,
    sessions: &ActorSessionMap,
    locks: &SpawnLocks,
    id: &str,
    only: Option<&str>,
) -> Result<(), String> {
    let room = store.get(id)?;
    let peers = room
        .participants
        .into_iter()
        .filter(|p| only.is_none_or(|x| x == p.id))
        .collect::<Vec<_>>();
    for p in peers {
        stop_peer(emitter, sessions, locks, p.run_id.clone()).await?;
        crate::rooms::runtime::import_events(store, id, &p)?;
        store.update(id, |r| {
            if let Some(x) = r.participants.iter_mut().find(|x| x.id == p.id) {
                x.pending_delivery = None;
                x.paused = p.paused;
                if !matches!(
                    x.state.as_str(),
                    "blocked" | "failed" | "completed" | "budget_exhausted"
                ) {
                    x.state = if x.paused || r.paused {
                        "paused".into()
                    } else {
                        "idle".into()
                    }
                }
            }
            Ok(())
        })?;
    }
    Ok(())
}

#[tauri::command]
pub async fn set_room_paused(
    store: State<'_, Arc<RoomStore>>,
    emitter: State<'_, Arc<BroadcastEmitter>>,
    sessions: State<'_, ActorSessionMap>,
    spawn_locks: State<'_, SpawnLocks>,
    id: String,
    paused: bool,
) -> Result<Room, String> {
    let before = store.get(&id)?;
    if before.archived && !paused {
        return Err("archived room cannot be resumed".into());
    }
    if before.paused == paused {
        return Ok(before);
    }
    store.update(&id, |r| {
        r.paused = paused;
        if !paused {
            for p in &mut r.participants {
                if !p.paused {
                    p.state = "idle".into();
                }
            }
        }
        Ok(())
    })?;
    if paused {
        stop_room_peers(
            &store,
            emitter.inner(),
            sessions.inner(),
            spawn_locks.inner(),
            &id,
            None,
        )
        .await?;
    }
    store.get(&id)
}
#[tauri::command]
pub fn post_room_message(
    store: State<'_, Arc<RoomStore>>,
    id: String,
    body: String,
    target_participant_id: Option<String>,
) -> Result<Room, String> {
    store.append_message(&id, "Human", body, None, target_participant_id, None)
}

fn validate_participant(input: &AddParticipantInput) -> Result<(), String> {
    if input.name.trim().is_empty() || input.name.trim().len() > 80 {
        return Err("participant name must contain 1–80 bytes".into());
    }
    if !matches!(input.provider.as_str(), "claude" | "codex") {
        return Err("provider must be claude or codex".into());
    }
    if !(1..=200).contains(&input.max_turns) {
        return Err("turn budget must be between 1 and 200".into());
    }
    if let Some(model) = input.model.as_deref() {
        if model.trim().is_empty() || model.len() > 200 || model.chars().any(char::is_control) {
            return Err("model must be a valid nonempty name of at most 200 bytes".into());
        }
    }
    if let Some(effort) = input.effort.as_deref() {
        let allowed = match input.provider.as_str() {
            "claude" => ["low", "medium", "high"].as_slice(),
            _ => ["minimal", "low", "medium", "high", "xhigh"].as_slice(),
        };
        if !allowed.contains(&effort) {
            return Err(format!("unsupported {} effort: {effort}", input.provider));
        }
    }
    Ok(())
}
#[tauri::command]
pub async fn add_room_participant(
    store: State<'_, Arc<RoomStore>>,
    id: String,
    input: AddParticipantInput,
) -> Result<Room, String> {
    validate_participant(&input)?;
    let room = store.get(&id)?;
    if room.archived {
        return Err("room is archived".into());
    }
    if room
        .participants
        .iter()
        .any(|p| p.name.eq_ignore_ascii_case(input.name.trim()))
    {
        return Err("participant name is already used in this room".into());
    }
    let peer_id = uuid::Uuid::new_v4().to_string();
    let run_id = uuid::Uuid::new_v4().to_string();
    let (cwd, worktree_path, branch) = if input.use_worktree {
        let (path, branch) = worktrees::create(&room.repo_path, &room.id, &peer_id).await?;
        (path.clone(), Some(path), Some(branch))
    } else {
        (room.repo_path.clone(), None, None)
    };
    let prompt=format!("You are a participant in the room '{}'. Objective: {}\nWork only on room tasks when assigned. Coordinate through the room tools.",room.title,room.objective);
    let mut meta = storage::runs::create_run(
        &run_id,
        &prompt,
        &cwd,
        &input.provider,
        RunStatus::Pending,
        input.model.clone(),
        None,
        None,
        None,
        None,
        None,
    )?;
    meta.execution_path = Some(ExecutionPath::SessionActor);
    meta.source = Some(RunSource::Native);
    meta.no_session_persistence = false;
    storage::runs::save_meta(&meta)?;
    let participant = Participant {
        id: peer_id.clone(),
        name: input.name.trim().into(),
        provider: input.provider,
        run_id: run_id.clone(),
        paused: true,
        model: input.model,
        effort: input.effort,
        worktree_path,
        branch,
        state: "paused".into(),
        last_error: None,
        last_wake_at: None,
        wake_count: 0,
        max_turns: input.max_turns,
        event_cursor: 0,
        message_cursor: 0,
        pending_delivery: None,
        no_progress_turns: 0,
        work_signature: None,
    };
    store.update(&id, |r| {
        if r.archived {
            return Err("room is archived".into());
        }
        if r.participants
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case(input.name.trim()))
        {
            return Err("participant name is already used in this room".into());
        }
        r.participants.push(participant.clone());
        Ok(())
    })?;
    store.get(&id)
}
#[tauri::command]
pub async fn set_room_participant_paused(
    store: State<'_, Arc<RoomStore>>,
    emitter: State<'_, Arc<BroadcastEmitter>>,
    sessions: State<'_, ActorSessionMap>,
    spawn_locks: State<'_, SpawnLocks>,
    id: String,
    participant_id: String,
    paused: bool,
) -> Result<Room, String> {
    let room = store.get(&id)?;
    let existing = room
        .participants
        .iter()
        .find(|p| p.id == participant_id)
        .ok_or("participant not found")?;
    if !paused {
        if !existing.paused {
            return Ok(room);
        }
        stop_peer(
            emitter.inner(),
            sessions.inner(),
            spawn_locks.inner(),
            existing.run_id.clone(),
        )
        .await?;
        crate::rooms::runtime::import_events(&store, &id, existing)?;
        return store.update(&id, |r| {
            let p = r
                .participants
                .iter_mut()
                .find(|p| p.id == participant_id)
                .ok_or("participant not found")?;
            p.paused = false;
            p.pending_delivery = None;
            p.wake_count = 0;
            p.no_progress_turns = 0;
            p.work_signature = None;
            p.last_error = None;
            p.state = "idle".into();
            Ok(())
        });
    }
    if existing.paused {
        return Ok(room);
    }
    store.update(&id, |r| {
        let p = r
            .participants
            .iter_mut()
            .find(|p| p.id == participant_id)
            .ok_or("participant not found")?;
        p.paused = paused;
        p.state = "paused".into();
        Ok(())
    })?;
    if paused {
        stop_room_peers(
            &store,
            emitter.inner(),
            sessions.inner(),
            spawn_locks.inner(),
            &id,
            Some(&participant_id),
        )
        .await?;
    }
    store.get(&id).or(Ok(room))
}
#[tauri::command]
pub fn wake_room_participant(
    store: State<'_, Arc<RoomStore>>,
    id: String,
    participant_id: String,
    message: String,
) -> Result<Room, String> {
    let room = store.get(&id)?;
    if room.paused || room.archived {
        return Err("room is paused or archived".into());
    }
    let p = room
        .participants
        .iter()
        .find(|p| p.id == participant_id)
        .ok_or("participant not found")?;
    if p.paused {
        return Err("resume the participant before sending a wake message".into());
    }
    if p.wake_count >= p.max_turns {
        return Err(
            "participant turn budget is exhausted; explicitly resume it to reset the budget".into(),
        );
    }
    let room = store.append_message(
        &id,
        "Human",
        message,
        None,
        Some(participant_id.clone()),
        None,
    )?;
    store.update(&id, |r| {
        let p = r
            .participants
            .iter_mut()
            .find(|p| p.id == participant_id)
            .ok_or("participant not found")?;
        p.last_wake_at = Some(chrono::Utc::now().timestamp_millis());
        Ok(())
    })?;
    store.get(&room.id)
}
#[tauri::command]
pub async fn remove_room_participant(
    store: State<'_, Arc<RoomStore>>,
    emitter: State<'_, Arc<BroadcastEmitter>>,
    sessions: State<'_, ActorSessionMap>,
    spawn_locks: State<'_, SpawnLocks>,
    id: String,
    participant_id: String,
) -> Result<Room, String> {
    let room = store.get(&id)?;
    if room.claims.iter().any(|c| {
        c.participant_id == participant_id && !matches!(c.state.as_str(), "done" | "released")
    }) {
        return Err("participant still owns an active task claim".into());
    }
    let peer = room
        .participants
        .iter()
        .find(|p| p.id == participant_id)
        .ok_or("participant not found")?
        .clone();
    store.update(&id, |r| {
        r.participants
            .iter_mut()
            .find(|p| p.id == participant_id)
            .ok_or("participant not found")?
            .paused = true;
        Ok(())
    })?;
    stop_peer(
        emitter.inner(),
        sessions.inner(),
        spawn_locks.inner(),
        peer.run_id,
    )
    .await?;
    store.update(&id, |r| {
        r.participants.retain(|p| p.id != participant_id);
        r.timers.retain(|t| t.participant_id != participant_id);
        Ok(())
    })
}

#[tauri::command]
pub fn save_room_timer(
    store: State<'_, Arc<RoomStore>>,
    id: String,
    input: SaveTimerInput,
) -> Result<Room, String> {
    if input.interval_seconds < 30 {
        return Err("timer interval must be at least 30 seconds".into());
    }
    if input.interval_seconds > 31_536_000 {
        return Err("timer interval cannot exceed one year".into());
    }
    if !(1..=200).contains(&input.max_deliveries) {
        return Err("timer delivery limit must be between 1 and 200".into());
    }
    if input.message.trim().is_empty() || input.message.len() > 32000 {
        return Err("timer message must contain 1–32000 bytes".into());
    }
    store.update(&id, |r| {
        if !r.participants.iter().any(|p| p.id == input.participant_id) {
            return Err("participant not found".into());
        }
        if let Some(timer_id) = input.id.as_deref() {
            let t = r
                .timers
                .iter_mut()
                .find(|t| t.id == timer_id)
                .ok_or("timer not found")?;
            t.participant_id = input.participant_id;
            t.message = input.message.trim().into();
            t.queued_at = None;
            t.interval_seconds = input.interval_seconds;
            t.idle_only = input.idle_only;
            t.enabled = input.enabled;
            t.max_deliveries = input.max_deliveries;
            t.next_due_at =
                chrono::Utc::now().timestamp_millis() + input.interval_seconds as i64 * 1000;
        } else {
            r.timers.push(Timer {
                id: uuid::Uuid::new_v4().to_string(),
                participant_id: input.participant_id,
                message: input.message.trim().into(),
                interval_seconds: input.interval_seconds,
                idle_only: input.idle_only,
                enabled: input.enabled,
                next_due_at: chrono::Utc::now().timestamp_millis()
                    + input.interval_seconds as i64 * 1000,
                max_deliveries: input.max_deliveries,
                delivered_count: 0,
                queued_at: None,
                last_error: None,
            })
        }
        Ok(())
    })
}
#[tauri::command]
pub fn remove_room_timer(
    store: State<'_, Arc<RoomStore>>,
    id: String,
    timer_id: String,
) -> Result<Room, String> {
    store.update(&id, |r| {
        let n = r.timers.len();
        r.timers.retain(|t| t.id != timer_id);
        if n == r.timers.len() {
            return Err("timer not found".into());
        }
        Ok(())
    })
}
#[tauri::command]
pub fn set_room_auto_continue(
    store: State<'_, Arc<RoomStore>>,
    id: String,
    enabled: bool,
) -> Result<Room, String> {
    store.update(&id, |r| {
        r.auto_continue = enabled;
        Ok(())
    })
}
#[tauri::command]
pub async fn attach_room_project(
    store: State<'_, Arc<RoomStore>>,
    id: String,
    number: u64,
) -> Result<Room, String> {
    let _op = store.project_operation.lock().await;
    let room = store.get(&id)?;
    if room.project.is_some() {
        return Err("room already has a GitHub Project".into());
    }
    let project = github_tasks::lookup_project(&room.repository, number).await?;
    github_tasks::prepare_project(&project).await?;
    store.update(&id, |r| {
        if r.project.is_some() {
            return Err("room already has a GitHub Project".into());
        }
        r.project = Some(project);
        r.project_stage = ProjectStage::Ready;
        Ok(())
    })
}
#[tauri::command]
pub async fn archive_room(
    store: State<'_, Arc<RoomStore>>,
    emitter: State<'_, Arc<BroadcastEmitter>>,
    sessions: State<'_, ActorSessionMap>,
    spawn_locks: State<'_, SpawnLocks>,
    id: String,
) -> Result<Room, String> {
    store.update(&id, |r| {
        r.paused = true;
        r.archived = true;
        Ok(())
    })?;
    stop_room_peers(
        &store,
        emitter.inner(),
        sessions.inner(),
        spawn_locks.inner(),
        &id,
        None,
    )
    .await?;
    store.get(&id)
}
#[tauri::command]
pub async fn release_room_claim(
    store: State<'_, Arc<RoomStore>>,
    emitter: State<'_, Arc<BroadcastEmitter>>,
    sessions: State<'_, ActorSessionMap>,
    spawn_locks: State<'_, SpawnLocks>,
    id: String,
    task_id: String,
) -> Result<Room, String> {
    let room = store.get(&id)?;
    let claim = room
        .claims
        .iter()
        .find(|c| c.task_id == task_id)
        .ok_or("claim not found")?;
    let peer = room
        .participants
        .iter()
        .find(|p| p.id == claim.participant_id)
        .ok_or("claim owner not found")?
        .clone();
    if !room.paused && !peer.paused {
        return Err("pause the claim owner or room before releasing the claim".into());
    }
    stop_peer(
        emitter.inner(),
        sessions.inner(),
        spawn_locks.inner(),
        peer.run_id.clone(),
    )
    .await?;
    store.update(&id, |r| {
        let p = r
            .participants
            .iter_mut()
            .find(|p| p.id == peer.id)
            .ok_or("participant not found")?;
        p.pending_delivery = None;
        Ok(())
    })?;
    crate::rooms::runtime::import_events(&store, &id, &peer)?;
    operations::release_claim(&store, &id, &task_id).await
}
#[tauri::command]
pub async fn merge_room_worktree(
    store: State<'_, Arc<RoomStore>>,
    emitter: State<'_, Arc<BroadcastEmitter>>,
    sessions: State<'_, ActorSessionMap>,
    spawn_locks: State<'_, SpawnLocks>,
    id: String,
    participant_id: String,
) -> Result<Room, String> {
    let room = store.get(&id)?;
    let peer = room
        .participants
        .iter()
        .find(|p| p.id == participant_id)
        .ok_or("participant not found")?
        .clone();
    if !room.paused && !peer.paused {
        return Err("pause the room or participant before merging".into());
    }
    let path = peer
        .worktree_path
        .clone()
        .ok_or("participant has no worktree")?;
    let branch = peer
        .branch
        .clone()
        .ok_or("participant has no worktree branch")?;
    stop_peer(
        emitter.inner(),
        sessions.inner(),
        spawn_locks.inner(),
        peer.run_id.clone(),
    )
    .await?;
    crate::rooms::runtime::import_events(&store, &id, &peer)?;
    store.update(&id, |r| {
        let p = r
            .participants
            .iter_mut()
            .find(|p| p.id == participant_id)
            .ok_or("participant not found")?;
        p.pending_delivery = None;
        p.state = "paused".into();
        Ok(())
    })?;
    worktrees::merge(&room.repo_path, &path, &branch).await?;
    store.get(&id)
}

#[tauri::command]
pub async fn read_room_task(
    store: State<'_, Arc<RoomStore>>,
    id: String,
    task_id: String,
) -> Result<serde_json::Value, String> {
    let room = store.get(&id)?;
    let project = room.project.ok_or("room has no GitHub Project")?;
    github_tasks::read_task(&project, &task_id).await
}

#[tauri::command]
pub fn set_room_concurrency(
    store: State<'_, Arc<RoomStore>>,
    id: String,
    limit: u32,
) -> Result<Room, String> {
    if !(1..=5).contains(&limit) {
        return Err("concurrent turn limit must be between 1 and 5".into());
    }
    store.update(&id, |r| {
        if r.archived {
            return Err("room is archived".into());
        }
        r.max_concurrent = limit;
        Ok(())
    })
}

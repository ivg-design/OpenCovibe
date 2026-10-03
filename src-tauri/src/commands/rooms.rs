use crate::{
    agent::{adapter::ActorSessionMap, spawn_locks::SpawnLocks},
    commands::session::stop_session_impl,
    models::{ExecutionPath, RunSource, RunStatus},
    rooms::{
        github, github_tasks, governance,
        models::{
            AddParticipantInput, CreateRoomInput, Participant, ParticipantSettings, ProjectStage,
            Room, RoomProject, SaveTimerInput, Timer,
        },
        operations, repository,
        store::RoomStore,
        worktrees,
    },
    storage,
    web_server::broadcaster::BroadcastEmitter,
};
use std::sync::Arc;
use tauri::State;

#[derive(serde::Serialize)]
pub struct RoomSessionSeed {
    run_id: String,
    title: String,
    objective: String,
    repo_path: String,
    repository: String,
    provider: String,
    source_cwd: String,
    existing_room_id: Option<String>,
    existing_room: Option<SessionRoomMatch>,
    projects: Vec<RoomProject>,
    project_error: Option<String>,
}

#[derive(serde::Serialize)]
pub struct SessionRoomMatch {
    id: String,
    title: String,
    repo_path: String,
    repository: String,
    source_attached: bool,
    participant_name: String,
    archived: bool,
}

async fn session_repository(cwd: &str) -> Result<(String, String), String> {
    let inspection = repository::inspect(cwd).await?;
    Ok((inspection.repo_path, inspection.repository))
}

#[tauri::command]
pub async fn inspect_room_repository(
    path: String,
) -> Result<repository::RepositoryInspection, String> {
    repository::inspect(&path).await
}

#[tauri::command]
pub async fn get_room_session_seed(
    store: State<'_, Arc<RoomStore>>,
    run_id: String,
    repo_path: Option<String>,
) -> Result<RoomSessionSeed, String> {
    let meta = storage::runs::get_run(&run_id).ok_or("Conversation not found")?;
    room_session_seed(&store, meta, repo_path).await
}

async fn room_session_seed(
    store: &RoomStore,
    meta: crate::models::RunMeta,
    selected_path: Option<String>,
) -> Result<RoomSessionSeed, String> {
    let run_id = meta.id.clone();
    let session_id = crate::rooms::session_seed::validate_source(&meta)?;
    let existing_room = store
        .list()?
        .into_iter()
        .find(|room| {
            room.participants.iter().any(|p| p.run_id == run_id)
                || room
                    .origin
                    .as_ref()
                    .is_some_and(|o| o.provider == meta.agent && o.session_id == session_id)
        })
        .map(|room| SessionRoomMatch {
            id: room.id.clone(),
            title: room.title.clone(),
            repo_path: room.repo_path.clone(),
            repository: room.repository.clone(),
            source_attached: room.participants.iter().any(|p| p.run_id == run_id),
            participant_name: crate::rooms::chat_attachment::former_peer(&room, &run_id)
                .map(|p| p.name)
                .unwrap_or_else(|| {
                    format!(
                        "Original {}",
                        if meta.agent == "codex" {
                            "Codex"
                        } else {
                            "Claude"
                        }
                    )
                }),
            archived: room.archived,
        });
    // A valid saved chat can originate in a general/non-Git folder. Keep its
    // identity visible and let the human choose the repository in setup.
    let (repo_path, repository) = session_repository(selected_path.as_deref().unwrap_or(&meta.cwd))
        .await
        .unwrap_or_default();
    let (projects, project_error) = if repository.is_empty() || existing_room.is_some() {
        (vec![], None)
    } else {
        match github::list_repository_projects(&repository).await {
            Ok(projects) => (projects, None),
            Err(error) => (vec![], Some(error)),
        }
    };
    Ok(RoomSessionSeed {
        run_id,
        title: crate::rooms::session_seed::source_title(&meta),
        objective: crate::rooms::session_seed::short_text(&meta.prompt, 12_000),
        repo_path,
        repository,
        provider: meta.agent,
        source_cwd: meta.cwd,
        existing_room_id: existing_room.as_ref().map(|room| room.id.clone()),
        existing_room,
        projects,
        project_error,
    })
}

#[tauri::command]
pub async fn create_room_from_session(
    store: State<'_, Arc<RoomStore>>,
    emitter: State<'_, Arc<BroadcastEmitter>>,
    sessions: State<'_, ActorSessionMap>,
    spawn_locks: State<'_, SpawnLocks>,
    run_id: String,
    mut input: CreateRoomInput,
    project_id: Option<String>,
) -> Result<Room, String> {
    let (room, create_project) = {
        let _guard = spawn_locks.acquire(&run_id).await;
        let mut meta = storage::runs::get_run(&run_id).ok_or("Conversation not found")?;
        let session_id = crate::rooms::session_seed::validate_source(&meta)?;
        if let Some(room) = store.list()?.into_iter().find(|room| {
            room.participants.iter().any(|p| p.run_id == run_id)
                || room
                    .origin
                    .as_ref()
                    .is_some_and(|o| o.provider == meta.agent && o.session_id == session_id)
        }) {
            return Err(format!(
                "This chat already has a room: {}. Open that room from session setup.",
                room.title
            ));
        }
        let inspection = repository::inspect(&input.repo_path).await?;
        input.repo_path = inspection.repo_path;
        // The selected folder is authoritative; require any supplied GitHub
        // repository to be one of its actual remotes, rather than silently
        // connecting an unrelated board.
        if !inspection.repositories.is_empty()
            && !inspection
                .repositories
                .iter()
                .any(|r| r.repository == input.repository)
        {
            return Err("The GitHub repository does not match the selected folder. Choose its detected remote.".into());
        }
        if input.title.trim().is_empty()
            || input.title.chars().count() > 120
            || input.objective.trim().is_empty()
            || input.objective.len() > 32_000
        {
            return Err(
                "Enter a room title (up to 120 characters) and objective (up to 32000 bytes)."
                    .into(),
            );
        }
        github::repository_parts(&input.repository)?;
        // Existing boards retain their fields and workflow; connecting is read-only.
        let imported_project = if let Some(project_id) = &project_id {
            let project = github::list_repository_projects(&input.repository)
                .await?
                .into_iter()
                .find(|project| &project.id == project_id)
                .ok_or("The selected Project is no longer linked to this repository. Refresh and choose again.")?;
            let board = github::read_board(&project.id).await?;
            Some((project, board))
        } else {
            None
        };
        // All history reads must succeed before changing the source actor or metadata.
        let seed_meta = meta.clone();
        let (origin, mut peer, messages) =
            tokio::task::spawn_blocking(move || crate::rooms::session_seed::build_seed(&seed_meta))
                .await
                .map_err(|e| e.to_string())??;
        if crate::commands::session::stop_actor(sessions.inner(), &run_id).await? {
            emitter.persist_and_emit(
                &run_id,
                &crate::models::BusEvent::RunState {
                    run_id: run_id.clone(),
                    state: "stopped".into(),
                    exit_code: None,
                    error: None,
                },
            );
            meta.status = RunStatus::Stopped;
        }
        // Restart on room delivery so the same provider thread receives room tools.
        meta.execution_path = Some(ExecutionPath::SessionActor);
        meta.cwd = input.repo_path.clone();
        storage::runs::save_meta(&meta)?;
        peer.effort = storage::settings::get_agent_settings(&meta.agent).effort;
        let create_project = input.create_project && imported_project.is_none();
        let room = store.create_from_session(input, origin, peer, messages)?;
        let room = if let Some((project, board)) = imported_project {
            store.update(&room.id, |room| {
                room.project = Some(project);
                room.board = board;
                room.project_stage = ProjectStage::Ready;
                Ok(())
            })?
        } else {
            room
        };
        (room, create_project)
    };
    if create_project {
        match ensure_project(&store, &room.id).await {
            Ok(room) => Ok(room),
            Err(_) => store.get(&room.id),
        }
    } else {
        Ok(room)
    }
}

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
                    "room_title": room.title,
                    "participant_id": participant.id,
                })
            })
    }))
}
#[tauri::command]
pub async fn create_room(
    store: State<'_, Arc<RoomStore>>,
    mut input: CreateRoomInput,
) -> Result<Room, String> {
    let inspected = repository::inspect(&input.repo_path).await?;
    input.repo_path = inspected.repo_path;
    if input.repository.trim().is_empty() && !inspected.repository.is_empty() {
        input.repository = inspected.repository;
    }
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
    let history_store = store.inner().clone();
    let history_id = id.clone();
    let room = tauri::async_runtime::spawn_blocking(move || {
        crate::rooms::runtime::reconcile_history(&history_store, &history_id)
    })
    .await
    .map_err(|e| e.to_string())??;
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
    sidechat_id: Option<String>,
    attachment_ids: Option<Vec<String>>,
) -> Result<Room, String> {
    store.append_message_with_attachments(
        &id,
        "Human",
        body,
        None,
        target_participant_id,
        None,
        sidechat_id,
        &attachment_ids.unwrap_or_default(),
    )
}

#[tauri::command]
pub async fn attach_room_files(
    store: State<'_, Arc<RoomStore>>,
    room_id: String,
    paths: Vec<String>,
) -> Result<Vec<crate::rooms::models::RoomAttachment>, String> {
    let store = store.inner().clone();
    tokio::task::spawn_blocking(move || store.attach_files(&room_id, &paths))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn upload_room_attachment(
    store: State<'_, Arc<RoomStore>>,
    room_id: String,
    name: String,
    content_base64: String,
) -> Result<crate::rooms::models::RoomAttachment, String> {
    let store = store.inner().clone();
    tokio::task::spawn_blocking(move || store.upload_attachment(&room_id, &name, &content_base64))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn read_room_attachment(
    store: State<'_, Arc<RoomStore>>,
    room_id: String,
    attachment_id: String,
) -> Result<crate::models::Attachment, String> {
    let store = store.inner().clone();
    tokio::task::spawn_blocking(move || store.read_attachment(&room_id, &attachment_id))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn open_room_attachment(
    store: State<'_, Arc<RoomStore>>,
    room_id: String,
    attachment_id: String,
) -> Result<(), String> {
    let (_, path) = store.attachment_path(&room_id, &attachment_id)?;
    // Reveal arbitrary file types; never execute a received binary or script.
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut c = tokio::process::Command::new("/usr/bin/open");
        c.arg("-R").arg(&path);
        c
    };
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut c = tokio::process::Command::new("explorer");
        c.arg(format!("/select,{}", path.display()));
        c
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = {
        let mut c = tokio::process::Command::new("xdg-open");
        c.arg(path.parent().ok_or("Attachment folder is missing.")?);
        c
    };
    let status = tokio::time::timeout(std::time::Duration::from_secs(10), command.status())
        .await
        .map_err(|_| "The file manager did not respond.".to_string())?
        .map_err(|_| "Cannot show this file in the file manager.".to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("Cannot show this file in the file manager.".into())
    }
}

#[tauri::command]
pub fn create_room_sidechat(
    store: State<'_, Arc<RoomStore>>,
    id: String,
    source_message_id: String,
    title: String,
    participant_ids: Option<Vec<String>>,
) -> Result<Room, String> {
    store.create_sidechat(
        &id,
        &source_message_id,
        &title,
        participant_ids.unwrap_or_default(),
    )
}

fn validate_participant(input: &AddParticipantInput) -> Result<(), String> {
    if input.name.trim().is_empty() || input.name.trim().len() > 80 {
        return Err("participant name must contain 1–80 bytes".into());
    }
    if !matches!(input.provider.as_str(), "claude" | "codex") {
        return Err("provider must be claude or codex".into());
    }
    if input.max_turns > 200 {
        return Err("turn limit must be between 1 and 200, or disabled".into());
    }
    if let Some(model) = input.model.as_deref() {
        if model.trim().is_empty() || model.len() > 200 || model.chars().any(char::is_control) {
            return Err("model must be a valid nonempty name of at most 200 bytes".into());
        }
    }
    if let Some(effort) = input.effort.as_deref() {
        let allowed = match input.provider.as_str() {
            "claude" => ["low", "medium", "high", "xhigh", "max"].as_slice(),
            _ => ["none", "minimal", "low", "medium", "high", "xhigh"].as_slice(),
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
    add_participant(&store, &id, input, None, None).await
}

#[tauri::command]
pub async fn list_attachable_room_chats(
    store: State<'_, Arc<RoomStore>>,
    id: String,
) -> Result<Vec<crate::rooms::chat_attachment::AttachableChat>, String> {
    use crate::rooms::chat_attachment::{
        former_peer, owns_attached_chat, owns_chat, validate_chat, AttachableChat,
    };
    let room = store.get(&id)?;
    let rooms = store.list()?;
    let common = repository::common_directory(&room.repo_path).await?;
    // Read only small metadata files: opening this picker never scans event logs.
    let metas = tokio::task::spawn_blocking(storage::runs::list_all_run_metas)
        .await
        .map_err(|e| e.to_string())?;
    let mut folders: std::collections::HashMap<String, Option<String>> =
        std::collections::HashMap::new();
    let mut chats = vec![];
    for meta in metas {
        let former = former_peer(&room, &meta.id);
        if validate_chat(&meta, former.is_some()).is_err()
            || owns_attached_chat(&room, &meta)
            || rooms.iter().any(|r| {
                (r.id != id || r.participants.iter().any(|p| p.run_id == meta.id))
                    && owns_chat(r, &meta)
            })
        {
            continue;
        }
        let folder_common = if let Some(value) = folders.get(&meta.cwd) {
            value.clone()
        } else {
            let value = repository::common_directory(&meta.cwd).await.ok();
            folders.insert(meta.cwd.clone(), value.clone());
            value
        };
        if folder_common.as_deref() != Some(common.as_str()) {
            continue;
        }
        chats.push(AttachableChat {
            run_id: meta.id.clone(),
            title: crate::rooms::session_seed::source_title(&meta),
            name: former
                .as_ref()
                .map(|p| p.name.clone())
                .unwrap_or_else(|| crate::rooms::session_seed::source_title(&meta)),
            provider: meta.agent.clone(),
            model: meta.model.clone(),
            cwd: meta.cwd.clone(),
            previous_participant: former.is_some(),
            started_at: meta.started_at.clone(),
        });
    }
    chats.sort_by(|a, b| {
        b.previous_participant
            .cmp(&a.previous_participant)
            .then_with(|| b.started_at.cmp(&a.started_at))
    });
    Ok(chats)
}

#[tauri::command]
pub async fn attach_room_chat(
    store: State<'_, Arc<RoomStore>>,
    sessions: State<'_, ActorSessionMap>,
    spawn_locks: State<'_, SpawnLocks>,
    id: String,
    run_id: String,
    name: String,
) -> Result<Room, String> {
    let _guard = spawn_locks.acquire(&run_id).await;
    let room = store.get(&id)?;
    if room.participants.iter().any(|p| p.run_id == run_id) {
        return Ok(room);
    }
    if room.archived {
        return Err("room is archived".into());
    }
    let mut meta = storage::runs::get_run(&run_id).ok_or("Chat not found")?;
    crate::rooms::chat_attachment::validate_chat(
        &meta,
        crate::rooms::chat_attachment::former_peer(&room, &run_id).is_some(),
    )?;
    let common = repository::common_directory(&room.repo_path).await?;
    if repository::common_directory(&meta.cwd).await? != common {
        return Err(
            "Choose a chat from this room's repository or one of its linked worktrees.".into(),
        );
    }
    if store
        .list()?
        .iter()
        .any(|r| r.id != id && crate::rooms::chat_attachment::owns_chat(r, &meta))
    {
        return Err("This chat already belongs to another room.".into());
    }
    let history_id = run_id.clone();
    let history =
        tokio::task::spawn_blocking(move || storage::history::get_summary(&history_id, true))
            .await
            .map_err(|e| e.to_string())??;
    let mut peer = crate::rooms::chat_attachment::restored_peer(
        &room,
        &meta,
        &name,
        history.last_seq,
        history.source_size,
    )?;
    let root = repository::inspect(&meta.cwd).await?.repo_path;
    if std::fs::canonicalize(&root).ok() != std::fs::canonicalize(&room.repo_path).ok() {
        peer.worktree_path = Some(root.clone());
        peer.branch = worktrees::current_branch(&root).await?;
    }
    // An idle standalone actor must restart with this room's tools on explicit resume.
    crate::commands::session::stop_actor(sessions.inner(), &run_id).await?;
    meta.execution_path = Some(ExecutionPath::SessionActor);
    meta.status = RunStatus::Stopped;
    storage::runs::save_meta(&meta)?;
    store.attach_chat(&id, peer, &meta)
}

async fn add_participant(
    store: &RoomStore,
    id: &str,
    input: AddParticipantInput,
    stable_id: Option<&str>,
    brief: Option<&str>,
) -> Result<Room, String> {
    validate_participant(&input)?;
    let room = store.get(id)?;
    if room.archived {
        return Err("room is archived".into());
    }
    let peer_id = stable_id
        .map(str::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    if let Some(peer) = room.participants.iter().find(|p| p.id == peer_id) {
        if peer.name != input.name.trim()
            || peer.provider != input.provider
            || peer.model != input.model
            || peer.effort != input.effort
            || peer.max_turns != input.max_turns
            || peer.worktree_path.is_some() != input.use_worktree
            || peer.brief.as_deref() != brief
        {
            return Err("request participant already exists with different settings".into());
        }
        return Ok(room);
    }
    let run_id = stable_id
        .map(str::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    if room
        .participants
        .iter()
        .any(|p| p.name.eq_ignore_ascii_case(input.name.trim()))
    {
        return Err("participant name is already used in this room".into());
    }
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
        RunStatus::Stopped,
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
        brief: brief.map(str::to_owned),
        worktree_path,
        branch,
        state: "paused".into(),
        last_error: None,
        last_wake_at: None,
        wake_count: 0,
        max_turns: input.max_turns,
        event_cursor: 0,
        event_offset: None,
        message_cursor: 0,
        pending_delivery: None,
        active_sidechat_id: None,
        read_message_ids: vec![],
        unread_message_ids: vec![],
        no_progress_turns: 0,
        work_signature: None,
    };
    store.update(id, |r| {
        if r.archived {
            return Err("room is archived".into());
        }
        if let Some(request_id) = stable_id {
            let request = r
                .requests
                .iter()
                .find(|request| request.id == request_id)
                .ok_or("agent request not found")?;
            if request.kind != "agent" || request.status != "creating" {
                return Err("agent approval was cancelled before the participant was added".into());
            }
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
    store.get(id)
}
/// Editing never grants work; a name-only edit does not stop an active provider.
#[tauri::command]
pub async fn update_room_participant_settings(
    store: State<'_, Arc<RoomStore>>,
    sessions: State<'_, ActorSessionMap>,
    spawn_locks: State<'_, SpawnLocks>,
    id: String,
    participant_id: String,
    input: ParticipantSettings,
    expected: ParticipantSettings,
) -> Result<Room, String> {
    let room = store.get(&id)?;
    let peer = room
        .participants
        .iter()
        .find(|p| p.id == participant_id)
        .ok_or("participant not found")?;
    validate_participant(&AddParticipantInput {
        name: input.name.clone(),
        provider: peer.provider.clone(),
        model: input.model.clone(),
        effort: input.effort.clone(),
        use_worktree: peer.worktree_path.is_some(),
        max_turns: input.max_turns,
    })?;
    let _guard = spawn_locks.acquire(&peer.run_id).await;
    // Validate before stopping the idle provider; then recheck inside the store transaction.
    operations::validate_settings_edit(&store.get(&id)?, &participant_id, &expected, &input)?;
    let execution_changed = !input.same_execution_settings(&expected);
    if execution_changed {
        crate::commands::session::stop_actor(sessions.inner(), &peer.run_id).await?;
    }
    store.update(&id, |room| {
        operations::apply_settings_edit(room, &participant_id, &input, &expected)?;
        if execution_changed {
            storage::runs::with_meta(&peer.run_id, |meta| {
                meta.model = input.model.clone();
                if meta.status == RunStatus::Running {
                    meta.status = RunStatus::Stopped;
                }
                Ok(())
            })?;
        }
        Ok(())
    })
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
        // Preserve an explicit human pause even if automatic task dormancy
        // already paused the peer. Fresh messages may wake blocked peers.
        return store.update(&id, |r| {
            let p = r
                .participants
                .iter_mut()
                .find(|p| p.id == participant_id)
                .ok_or("participant not found")?;
            p.paused = true;
            p.state = "paused".into();
            Ok(())
        });
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
    if p.turn_limit_reached() {
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
        peer.run_id.clone(),
    )
    .await?;
    store.update(&id, |r| {
        if r.claims.iter().any(|c| {
            c.participant_id == participant_id && !matches!(c.state.as_str(), "done" | "released")
        }) {
            return Err("participant still owns an active task claim".into());
        }
        if let Some(mut detached) = r
            .participants
            .iter()
            .find(|p| p.id == participant_id)
            .cloned()
        {
            detached.paused = true;
            detached.state = "paused".into();
            detached.pending_delivery = None;
            r.detached_participants
                .retain(|p| p.run_id != detached.run_id);
            r.detached_participants.push(detached);
        }
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
    validate_timer_limit(
        input.max_deliveries,
        input.ends_at,
        chrono::Utc::now().timestamp_millis(),
    )?;
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
            t.ends_at = input.ends_at;
            t.next_due_at = chrono::Utc::now()
                .timestamp_millis()
                .saturating_add((input.interval_seconds as i64).saturating_mul(1000));
        } else {
            r.timers.push(Timer {
                id: uuid::Uuid::new_v4().to_string(),
                participant_id: input.participant_id,
                message: input.message.trim().into(),
                interval_seconds: input.interval_seconds,
                idle_only: input.idle_only,
                enabled: input.enabled,
                next_due_at: chrono::Utc::now()
                    .timestamp_millis()
                    .saturating_add((input.interval_seconds as i64).saturating_mul(1000)),
                max_deliveries: input.max_deliveries,
                ends_at: input.ends_at,
                delivered_count: 0,
                queued_at: None,
                last_error: None,
            })
        }
        Ok(())
    })
}

fn validate_timer_limit(
    max_deliveries: Option<u32>,
    ends_at: Option<i64>,
    now_ms: i64,
) -> Result<(), String> {
    match (max_deliveries, ends_at) {
        (Some(count), None) if (1..=200).contains(&count) => Ok(()),
        (Some(_), None) => Err("timer delivery limit must be between 1 and 200".into()),
        (None, Some(end)) if end > now_ms => Ok(()),
        (None, Some(_)) => Err("timer end date must be in the future".into()),
        _ => Err("set exactly one timer limit: a delivery count or an end date".into()),
    }
}

#[cfg(test)]
mod timer_tests {
    use super::validate_timer_limit;

    #[test]
    fn timer_count_and_date_limits_validate_exactly_one_future_limit() {
        let now = 10_000;
        assert!(validate_timer_limit(Some(1), None, now).is_ok());
        assert!(validate_timer_limit(Some(200), None, now).is_ok());
        assert!(validate_timer_limit(Some(0), None, now).is_err());
        assert!(validate_timer_limit(Some(201), None, now).is_err());
        assert!(validate_timer_limit(None, Some(now + 1), now).is_ok());
        assert!(validate_timer_limit(None, Some(now), now).is_err());
        assert!(validate_timer_limit(None, Some(now - 1), now).is_err());
        assert!(validate_timer_limit(None, None, now).is_err());
        assert!(validate_timer_limit(Some(5), Some(now + 1), now).is_err());
    }
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
    github_tasks::read_task_with_progress(&project, &task_id).await
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

#[tauri::command]
pub fn save_room_title(
    store: State<'_, Arc<RoomStore>>,
    id: String,
    title: String,
    expected: String,
) -> Result<Room, String> {
    store.save_title(&id, title, expected)
}
#[tauri::command]
pub fn save_room_instructions(
    store: State<'_, Arc<RoomStore>>,
    id: String,
    instructions: String,
    expected: String,
) -> Result<Room, String> {
    store.save_instructions(&id, instructions, expected)
}

#[tauri::command]
pub async fn resolve_room_request(
    store: State<'_, Arc<RoomStore>>,
    id: String,
    request_id: String,
    approve: bool,
    response: String,
) -> Result<Room, String> {
    let _operation = store.project_operation.lock().await;
    governance::resolve_request(&store, &id, &request_id, approve, &response)
}

#[tauri::command]
pub async fn close_room_request(
    store: State<'_, Arc<RoomStore>>,
    id: String,
    request_id: String,
    reason: String,
) -> Result<Room, String> {
    let _operation = store.project_operation.lock().await;
    governance::close_request(&store, &id, None, &request_id, &reason)
}

#[tauri::command]
pub fn archive_room_requests(
    store: State<'_, Arc<RoomStore>>,
    id: String,
    request_ids: Vec<String>,
    archived: bool,
) -> Result<Room, String> {
    governance::archive_requests(&store, &id, None, &request_ids, archived)
}

#[tauri::command]
pub async fn approve_room_agent(
    store: State<'_, Arc<RoomStore>>,
    id: String,
    request_id: String,
) -> Result<Room, String> {
    let _operation = store.project_operation.lock().await;
    let room = store.get(&id)?;
    let request = room
        .requests
        .iter()
        .find(|r| r.id == request_id)
        .ok_or("request not found")?;
    if request.kind != "agent" {
        return Err("request is not for an additional agent".into());
    }
    if request.status == "approved" {
        return Ok(room);
    }
    if room.archived || !matches!(request.status.as_str(), "pending" | "creating") {
        return Err("agent request is no longer pending".into());
    }
    let proposal = request
        .proposal
        .clone()
        .ok_or("request has no proposed participant")?;
    let brief = request.brief.clone().ok_or("request has no brief")?;
    validate_participant(&proposal)?;
    store.update(&id, |room| {
        if room.archived {
            return Err("room is archived".into());
        }
        let request = room
            .requests
            .iter_mut()
            .find(|r| r.id == request_id)
            .ok_or("request not found")?;
        if !matches!(request.status.as_str(), "pending" | "creating") {
            return Err("agent request was already resolved".into());
        }
        request.status = "creating".into();
        request.updated_at = crate::models::now_iso();
        Ok(())
    })?;
    if let Err(error) =
        add_participant(&store, &id, proposal, Some(&request_id), Some(&brief)).await
    {
        store.update(&id, |room| {
            if let Some(request) = room.requests.iter_mut().find(|r| r.id == request_id) {
                if request.status == "creating" { request.response = Some(format!("Adding the paused peer failed: {error}. Retry uses the same identity and preserves any created worktree.")); }
            }
            Ok(())
        })?;
        return Err(error);
    }
    governance::record_agent_approval(&store, &id, &request_id, &request_id)
}

#[derive(serde::Serialize)]
pub struct RoomAgentIdentity {
    room_id: String,
    run_id: String,
    participant_id: String,
    name: String,
    color_index: usize,
}

#[tauri::command]
pub fn list_room_agent_identities(
    store: State<'_, Arc<RoomStore>>,
) -> Result<Vec<RoomAgentIdentity>, String> {
    Ok(store
        .list()?
        .into_iter()
        .flat_map(|room| {
            let room_id = room.id;
            room.participants
                .into_iter()
                .enumerate()
                .map(move |(color_index, peer)| RoomAgentIdentity {
                    room_id: room_id.clone(),
                    run_id: peer.run_id,
                    participant_id: peer.id,
                    name: peer.name,
                    color_index,
                })
        })
        .collect())
}

#[derive(serde::Serialize)]
pub struct RoomSidebarEntry {
    paused: bool,
    id: String,
    title: String,
    repo_path: String,
    updated_at: String,
    needs_answer: usize,
    participants: Vec<RoomSidebarPeer>,
}

#[derive(serde::Serialize)]
pub struct RoomSidebarPeer {
    run_id: String,
    participant_id: String,
    name: String,
    provider: String,
    state: String,
    color_index: usize,
}

#[tauri::command]
pub async fn list_room_sidebar_entries(
    store: State<'_, Arc<RoomStore>>,
) -> Result<Vec<RoomSidebarEntry>, String> {
    let store = Arc::clone(&store);
    tauri::async_runtime::spawn_blocking(move || {
        Ok(store
            .list()?
            .into_iter()
            .filter(|room| !room.archived)
            .map(|room| {
                let needs_answer = room
                    .requests
                    .iter()
                    .filter(|request| {
                        (request.status == "pending"
                            && matches!(request.kind.as_str(), "agent" | "decision"))
                            || (request.kind == "completion" && request.status == "verified")
                    })
                    .count();
                RoomSidebarEntry {
                    paused: room.paused,
                    id: room.id,
                    title: room.title,
                    repo_path: room.repo_path,
                    updated_at: room.updated_at,
                    needs_answer,
                    participants: room
                        .participants
                        .into_iter()
                        .enumerate()
                        .map(|(color_index, peer)| RoomSidebarPeer {
                            run_id: peer.run_id,
                            participant_id: peer.id,
                            name: peer.name,
                            provider: peer.provider,
                            state: peer.state,
                            color_index,
                        })
                        .collect(),
                }
            })
            .collect())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn get_room_clipboard_paths() -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        crate::commands::clipboard::get_clipboard_files()
            .map(|files| files.into_iter().map(|file| file.path).collect())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod session_setup_tests {
    use super::*;
    #[tokio::test]
    async fn general_folder_session_stays_in_setup_and_names_existing_destination() {
        let tmp = tempfile::tempdir().unwrap();
        let store = RoomStore::open(&tmp.path().join("rooms.db")).unwrap();
        let meta: crate::models::RunMeta = serde_json::from_value(serde_json::json!({
            "id":"general-source", "name":"Huion work", "prompt":"Continue wheel fixes", "cwd": tmp.path(),
            "agent":"codex", "status":"stopped", "started_at":"2026-10-02", "session_id":"saved-thread"
        })).unwrap();
        let seed = room_session_seed(&store, meta.clone(), None).await.unwrap();
        assert_eq!(seed.title, "Huion work");
        assert_eq!(seed.source_cwd, meta.cwd);
        assert!(seed.repo_path.is_empty());
        assert!(seed.existing_room.is_none());
        assert!(store.list().unwrap().is_empty());
        let room = store
            .create_from_session(
                CreateRoomInput {
                    title: "KDCustom room".into(),
                    objective: "Work".into(),
                    repo_path: meta.cwd.clone(),
                    repository: "acme/test".into(),
                    create_project: false,
                },
                crate::rooms::models::RoomOrigin {
                    run_id: meta.id.clone(),
                    provider: meta.agent.clone(),
                    session_id: "saved-thread".into(),
                    title: seed.title,
                    message_count: 0,
                    context: "Context".into(),
                },
                Participant {
                    id: "original".into(),
                    name: "Original Codex".into(),
                    run_id: meta.id.clone(),
                    paused: true,
                    ..Default::default()
                },
                vec![],
            )
            .unwrap();
        let seed = room_session_seed(&store, meta.clone(), None).await.unwrap();
        let matched = seed.existing_room.unwrap();
        assert_eq!(matched.id, room.id);
        assert_eq!(matched.title, "KDCustom room");
        assert!(matched.source_attached);
        store
            .update(&room.id, |r| {
                r.participants.clear();
                Ok(())
            })
            .unwrap();
        let seed = room_session_seed(&store, meta, None).await.unwrap();
        assert!(!seed.existing_room.unwrap().source_attached);
        assert_eq!(store.list().unwrap().len(), 1);
        assert_eq!(store.get(&room.id).unwrap().participants.len(), 0);
    }
}

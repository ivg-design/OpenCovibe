use crate::rooms::{
    github,
    models::{CreateRoomInput, ProjectStage, Room},
    store::RoomStore,
};
use tauri::State;

#[tauri::command]
pub fn list_rooms(store: State<'_, RoomStore>) -> Result<Vec<Room>, String> {
    store.list()
}

#[tauri::command]
pub fn get_room(store: State<'_, RoomStore>, id: String) -> Result<Room, String> {
    store.get(&id)
}

#[tauri::command]
pub async fn create_room(
    store: State<'_, RoomStore>,
    input: CreateRoomInput,
) -> Result<Room, String> {
    let create_project = input.create_project;
    let room = store.create(input)?;
    if create_project {
        ensure_project(&store, &room.id).await
    } else {
        Ok(room)
    }
}

async fn ensure_project(store: &RoomStore, id: &str) -> Result<Room, String> {
    let _operation = store.project_operation.lock().await;
    let room = store.get(id)?;
    if room.project.is_some() {
        return Ok(room);
    }
    let outcome = async {
        let (repo_id, owner_id) = github::repository_ids(&room.repository).await?;
        if let Some(project) = github::find_project(&owner_id, &room.repository, &room.project_title()).await? {
            return Ok(project);
        }
        if room.project_stage != ProjectStage::NotStarted {
            return Err("Previous Project creation is unconfirmed. Refresh to reconcile it; a duplicate will not be created.".to_string());
        }
        store.update(id, |r| { r.project_stage = ProjectStage::Creating; Ok(()) })?;
        github::create_project(&room.repository, &repo_id, &owner_id, &room.project_title()).await
    }.await;
    match outcome {
        Ok(project) => store.update(id, |room| {
            room.project = Some(project);
            room.project_stage = ProjectStage::Ready;
            room.board.error = None;
            Ok(())
        }),
        Err(error) => store.update(id, |room| {
            if room.project_stage == ProjectStage::Creating {
                room.project_stage = if error.starts_with("GitHub rejected request:") {
                    ProjectStage::NotStarted
                } else {
                    ProjectStage::Uncertain
                };
            }
            room.board.error = Some(error);
            Ok(())
        }),
    }
}

#[tauri::command]
pub async fn ensure_room_project(store: State<'_, RoomStore>, id: String) -> Result<Room, String> {
    ensure_project(&store, &id).await
}

#[tauri::command]
pub async fn refresh_room_board(store: State<'_, RoomStore>, id: String) -> Result<Room, String> {
    let _operation = store.project_operation.lock().await;
    let room = store.get(&id)?;
    let project = room.project.ok_or("This room has no GitHub Project yet")?;
    match github::read_board(&project.id).await {
        Ok(board) => store.update(&id, |room| {
            room.board = board;
            Ok(())
        }),
        Err(error) => store.update(&id, |room| {
            room.board.error = Some(error);
            Ok(())
        }),
    }
}

#[tauri::command]
pub fn set_room_paused(
    store: State<'_, RoomStore>,
    id: String,
    paused: bool,
) -> Result<Room, String> {
    store.update(&id, |room| {
        room.paused = paused;
        Ok(())
    })
}

#[tauri::command]
pub fn post_room_message(
    store: State<'_, RoomStore>,
    id: String,
    body: String,
) -> Result<Room, String> {
    store.post_message(&id, body)
}

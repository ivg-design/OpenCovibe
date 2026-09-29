use super::models::{Board, CreateRoomInput, Message, ProjectStage, Room};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::path::Path;
use std::sync::Mutex;

pub struct RoomStore {
    connection: Mutex<Connection>,
    pub project_operation: tokio::sync::Mutex<()>,
}

impl RoomStore {
    pub fn open(path: &Path) -> Result<Self, String> {
        let parent = path
            .parent()
            .ok_or("room database needs a parent directory")?;
        crate::storage::ensure_dir(parent).map_err(|e| e.to_string())?;
        let connection = Connection::open(path).map_err(|e| e.to_string())?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
                 CREATE TABLE IF NOT EXISTS rooms (
                   id TEXT PRIMARY KEY NOT NULL, payload TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS task_creation_intents (
                   room_id TEXT NOT NULL, title TEXT NOT NULL, body TEXT NOT NULL,
                   task_id TEXT, PRIMARY KEY(room_id, title)
                 );",
            )
            .map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| e.to_string())?;
        }
        Ok(Self {
            connection: Mutex::new(connection),
            project_operation: tokio::sync::Mutex::new(()),
        })
    }

    pub fn create(&self, input: CreateRoomInput) -> Result<Room, String> {
        let title = input.title.trim();
        let objective = input.objective.trim();
        if title.is_empty() || title.chars().count() > 120 {
            return Err("room title must contain 1–120 characters".into());
        }
        if objective.is_empty() || objective.len() > 32_000 {
            return Err("room objective must contain 1–32000 bytes".into());
        }
        super::github::repository_parts(&input.repository)?;
        let path = Path::new(&input.repo_path);
        if !path.is_absolute() || !path.is_dir() {
            return Err("choose an existing absolute local repository directory".into());
        }
        let now = crate::models::now_iso();
        let room = Room {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.into(),
            objective: objective.into(),
            repo_path: path
                .canonicalize()
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .into(),
            repository: input.repository.trim().into(),
            created_at: now.clone(),
            updated_at: now,
            paused: true,
            project: None,
            project_stage: ProjectStage::NotStarted,
            board: Board::default(),
            participants: vec![],
            messages: vec![],
            timers: vec![],
            claims: vec![],
            auto_continue: true,
            archived: false,
            runtime_error: None,
        };
        let conn = self.connection.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO rooms (id, payload) VALUES (?1, ?2)",
            params![
                room.id,
                serde_json::to_string(&room).map_err(|e| e.to_string())?
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(room)
    }

    pub fn list(&self) -> Result<Vec<Room>, String> {
        let conn = self.connection.lock().map_err(|e| e.to_string())?;
        let mut statement = conn
            .prepare("SELECT payload FROM rooms")
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        let mut rooms = vec![];
        for row in rows {
            rooms.push(
                serde_json::from_str::<Room>(&row.map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?,
            );
        }
        rooms.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(rooms)
    }

    pub fn get(&self, id: &str) -> Result<Room, String> {
        let conn = self.connection.lock().map_err(|e| e.to_string())?;
        let payload: String = conn
            .query_row("SELECT payload FROM rooms WHERE id=?1", [id], |row| {
                row.get(0)
            })
            .map_err(|e| format!("room not found or unreadable: {e}"))?;
        serde_json::from_str(&payload).map_err(|e| e.to_string())
    }

    pub fn update(
        &self,
        id: &str,
        change: impl FnOnce(&mut Room) -> Result<(), String>,
    ) -> Result<Room, String> {
        let mut conn = self.connection.lock().map_err(|e| e.to_string())?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        let payload: String = tx
            .query_row("SELECT payload FROM rooms WHERE id=?1", [id], |row| {
                row.get(0)
            })
            .map_err(|e| format!("room not found or unreadable: {e}"))?;
        let mut room: Room = serde_json::from_str(&payload).map_err(|e| e.to_string())?;
        change(&mut room)?;
        room.updated_at = crate::models::now_iso();
        tx.execute(
            "UPDATE rooms SET payload=?1 WHERE id=?2",
            params![serde_json::to_string(&room).map_err(|e| e.to_string())?, id],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(room)
    }

    pub fn post_message(&self, id: &str, body: String) -> Result<Room, String> {
        self.append_message(id, "Human", body, None, None, None)
    }

    pub fn append_message(
        &self,
        id: &str,
        sender: &str,
        body: String,
        participant_id: Option<String>,
        target_participant_id: Option<String>,
        source_event_id: Option<String>,
    ) -> Result<Room, String> {
        let body = body.trim();
        if body.is_empty() || body.len() > 32_000 {
            return Err("message must contain 1–32000 bytes".into());
        }
        self.update(id, |room| {
            if target_participant_id
                .as_ref()
                .is_some_and(|target| !room.participants.iter().any(|p| &p.id == target))
            {
                return Err("target participant is not in this room".into());
            }
            if source_event_id.as_ref().is_some_and(|source| {
                room.messages
                    .iter()
                    .any(|m| m.source_event_id.as_ref() == Some(source))
            }) {
                return Ok(());
            }
            room.messages.push(Message {
                id: uuid::Uuid::new_v4().to_string(),
                sender: sender.into(),
                body: body.into(),
                created_at: crate::models::now_iso(),
                participant_id,
                target_participant_id,
                source_event_id,
            });
            Ok(())
        })
    }

    pub fn reserve_claim(
        &self,
        id: &str,
        participant_id: &str,
        task_id: &str,
    ) -> Result<Room, String> {
        self.update(id, |room| {
            let peer = room
                .participants
                .iter()
                .find(|p| p.id == participant_id)
                .ok_or("participant not found")?;
            if room.paused
                || room.archived
                || peer.paused
                || (peer.wake_count >= peer.max_turns && peer.pending_delivery.is_none())
            {
                return Err("room or participant is paused or its turn budget is exhausted".into());
            }
            let now = chrono::Utc::now().timestamp_millis();
            let fresh = room
                .board
                .synced_at
                .as_deref()
                .and_then(|ts| chrono::DateTime::parse_from_rfc3339(ts).ok())
                .is_some_and(|ts| {
                    (0..=120_000).contains(&now.saturating_sub(ts.timestamp_millis()))
                });
            if room.board.error.is_some() || !fresh {
                return Err("refresh the board before claiming work".into());
            }
            if !super::scheduler::eligible_tasks(&room.board, peer)
                .iter()
                .any(|id| id == task_id)
            {
                return Err("task is not eligible for this participant".into());
            }
            if room.claims.iter().any(|c| {
                c.participant_id == participant_id
                    && c.task_id != task_id
                    && !matches!(c.state.as_str(), "done" | "released")
            }) {
                return Err("finish or release this participant's existing task first".into());
            }
            if let Some(claim) = room.claims.iter_mut().find(|c| c.task_id == task_id) {
                if !matches!(claim.state.as_str(), "done" | "released") {
                    if claim.participant_id != participant_id {
                        return Err("task is already claimed by another participant".into());
                    }
                    if !matches!(claim.state.as_str(), "active" | "reserved") {
                        return Err("task claim needs explicit recovery or release".into());
                    }
                    return Ok(());
                }
                claim.participant_id = participant_id.into();
                claim.state = "reserved".into();
                claim.summary = None;
                claim.evidence = None;
                claim.updated_at = crate::models::now_iso();
            } else {
                room.claims.push(super::models::Claim {
                    task_id: task_id.into(),
                    participant_id: participant_id.into(),
                    state: "reserved".into(),
                    updated_at: crate::models::now_iso(),
                    summary: None,
                    evidence: None,
                });
            }
            Ok(())
        })
    }
    /// Returns true only to the first writer. Interrupted intents require remote reconciliation.
    pub fn begin_task_creation(
        &self,
        room_id: &str,
        title: &str,
        body: &str,
    ) -> Result<bool, String> {
        let mut conn = self.connection.lock().map_err(|e| e.to_string())?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        let existing: Option<String> = tx
            .query_row(
                "SELECT body FROM task_creation_intents WHERE room_id=?1 AND title=?2",
                params![room_id, title],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some(existing) = existing {
            if existing != body {
                return Err(
                    "A task creation with this title has a different body; choose a distinct title"
                        .into(),
                );
            }
            return Ok(false);
        }
        tx.execute(
            "INSERT INTO task_creation_intents(room_id,title,body) VALUES(?1,?2,?3)",
            params![room_id, title, body],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(true)
    }

    pub fn created_task_id(&self, room_id: &str, title: &str) -> Result<Option<String>, String> {
        let conn = self.connection.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT task_id FROM task_creation_intents WHERE room_id=?1 AND title=?2",
            params![room_id, title],
            |r| r.get(0),
        )
        .optional()
        .map(|id: Option<Option<String>>| id.flatten())
        .map_err(|e| e.to_string())
    }

    pub fn complete_task_creation(
        &self,
        room_id: &str,
        title: &str,
        task_id: &str,
    ) -> Result<(), String> {
        let conn = self.connection.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE task_creation_intents SET task_id=?1 WHERE room_id=?2 AND title=?3",
            params![task_id, room_id, title],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(path: &Path) -> CreateRoomInput {
        CreateRoomInput {
            title: "Test room".into(),
            objective: "Finish a bounded task".into(),
            repo_path: path.to_string_lossy().into(),
            repository: "ivg-design/OpenCovibe".into(),
            create_project: false,
        }
    }

    #[test]
    fn room_and_notes_survive_reopen_without_creating_a_second_room() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("rooms.db");
        let store = RoomStore::open(&path).unwrap();
        let room = store.create(input(temp.path())).unwrap();
        store
            .post_message(&room.id, "Keep the same board".into())
            .unwrap();
        drop(store);
        let store = RoomStore::open(&path).unwrap();
        assert_eq!(store.list().unwrap().len(), 1);
        let recovered = store.get(&room.id).unwrap();
        assert!(recovered.paused);
        assert_eq!(recovered.messages[0].body, "Keep the same board");
        assert_eq!(recovered.project_title(), room.project_title());
    }

    #[test]
    fn rejected_update_rolls_back_and_message_inputs_are_bounded() {
        let temp = tempfile::tempdir().unwrap();
        let store = RoomStore::open(&temp.path().join("rooms.db")).unwrap();
        let room = store.create(input(temp.path())).unwrap();
        assert!(store
            .update(&room.id, |r| {
                r.title = "bad".into();
                Err("reject".into())
            })
            .is_err());
        assert_eq!(store.get(&room.id).unwrap().title, "Test room");
        assert!(store.post_message(&room.id, " ".into()).is_err());
        assert!(store.post_message(&room.id, "a".repeat(32_001)).is_err());
    }

    #[test]
    fn two_connections_preserve_independent_updates() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("rooms.db");
        let a = RoomStore::open(&path).unwrap();
        let b = RoomStore::open(&path).unwrap();
        let room = a.create(input(temp.path())).unwrap();
        a.post_message(&room.id, "first".into()).unwrap();
        b.post_message(&room.id, "second".into()).unwrap();
        assert_eq!(a.get(&room.id).unwrap().messages.len(), 2);
    }
}

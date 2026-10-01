use super::models::{
    Board, CreateRoomInput, Message, Participant, ProjectStage, Room, RoomOrigin, Sidechat,
};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::path::Path;
use std::sync::Mutex;

pub struct RoomStore {
    connection: Mutex<Connection>,
    pub(crate) attachment_root: std::path::PathBuf,
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
            attachment_root: parent.join("room-attachments"),
            project_operation: tokio::sync::Mutex::new(()),
        })
    }

    pub fn create(&self, input: CreateRoomInput) -> Result<Room, String> {
        self.create_seeded(input, None)
    }

    pub fn create_from_session(
        &self,
        input: CreateRoomInput,
        origin: RoomOrigin,
        peer: Participant,
        messages: Vec<Message>,
    ) -> Result<Room, String> {
        self.create_seeded(input, Some((origin, peer, messages)))
    }

    fn create_seeded(
        &self,
        input: CreateRoomInput,
        seed: Option<(RoomOrigin, Participant, Vec<Message>)>,
    ) -> Result<Room, String> {
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
        let mut room = Room {
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
            sidechats: vec![],
            timers: vec![],
            claims: vec![],
            requests: vec![],
            auto_continue: true,
            max_concurrent: 3,
            archived: false,
            runtime_error: None,
            origin: None,
        };
        if let Some((origin, peer, messages)) = seed {
            room.origin = Some(origin);
            room.participants.push(peer);
            room.messages = messages;
        }
        let mut conn = self.connection.lock().map_err(|e| e.to_string())?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        if let Some(origin) = &room.origin {
            let existing = {
                let mut statement = tx
                    .prepare("SELECT payload FROM rooms")
                    .map_err(|e| e.to_string())?;
                let rows = statement
                    .query_map([], |row| row.get::<_, String>(0))
                    .map_err(|e| e.to_string())?;
                let mut found = None;
                for row in rows {
                    let saved: Room = serde_json::from_str(&row.map_err(|e| e.to_string())?)
                        .map_err(|e| e.to_string())?;
                    if saved.participants.iter().any(|p| p.run_id == origin.run_id)
                        || saved.origin.as_ref().is_some_and(|o| {
                            o.provider == origin.provider && o.session_id == origin.session_id
                        })
                    {
                        found = Some(saved);
                        break;
                    }
                }
                found
            };
            if let Some(existing) = existing {
                return Ok(existing);
            }
        }
        tx.execute(
            "INSERT INTO rooms (id, payload) VALUES (?1, ?2)",
            params![
                room.id,
                serde_json::to_string(&room).map_err(|e| e.to_string())?
            ],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
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

    pub fn save_instructions(
        &self,
        id: &str,
        instructions: String,
        expected: String,
    ) -> Result<Room, String> {
        let instructions = instructions.trim();
        if instructions.is_empty() || instructions.len() > 32_000 {
            return Err("Enter room instructions (up to 32000 bytes).".into());
        }
        self.update(id, |room| {
            if room.archived {
                return Err("room is archived".into());
            }
            if room.objective != expected {
                return Err("Room instructions changed elsewhere. Cancel and reopen the editor to review the latest version.".into());
            }
            room.objective = instructions.to_owned();
            Ok(())
        })
    }

    pub fn post_message(&self, id: &str, body: String) -> Result<Room, String> {
        self.append_message(id, "Human", body, None, None, None)
    }

    // A network read must not overwrite a task write or a newer refresh that completed
    // while that read was in flight. This comparison also works across MCP processes.
    pub fn apply_board_snapshot(
        &self,
        id: &str,
        expected: &Board,
        board: Board,
    ) -> Result<Room, String> {
        self.update(id, |room| {
            if room.board == *expected {
                room.board = board;
                room.runtime_error = None;
            }
            Ok(())
        })
    }

    pub fn apply_board_error(
        &self,
        id: &str,
        expected: &Board,
        error: String,
    ) -> Result<Room, String> {
        self.update(id, |room| {
            if room.board == *expected {
                room.board.error = Some(error);
            }
            Ok(())
        })
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
        self.append_message_in_sidechat(
            id,
            sender,
            body,
            participant_id,
            target_participant_id,
            source_event_id,
            None,
        )
    }

    // Retain the existing message entry point while adding an optional conversation scope.
    #[allow(clippy::too_many_arguments)]
    pub fn append_message_in_sidechat(
        &self,
        id: &str,
        sender: &str,
        body: String,
        participant_id: Option<String>,
        target_participant_id: Option<String>,
        source_event_id: Option<String>,
        sidechat_id: Option<String>,
    ) -> Result<Room, String> {
        self.append_message_with_attachments(
            id,
            sender,
            body,
            participant_id,
            target_participant_id,
            source_event_id,
            sidechat_id,
            &[],
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn append_message_with_attachments(
        &self,
        id: &str,
        sender: &str,
        body: String,
        participant_id: Option<String>,
        target_participant_id: Option<String>,
        source_event_id: Option<String>,
        sidechat_id: Option<String>,
        attachment_ids: &[String],
    ) -> Result<Room, String> {
        let attachments = self.resolve_attachments(id, attachment_ids)?;
        let body = body.trim();
        if (body.is_empty() && attachments.is_empty()) || body.len() > 32_000 {
            return Err(
                "Add a message or attachment. Messages can contain up to 32000 bytes.".into(),
            );
        }
        self.update(id, |room| {
            if room.archived {
                return Err("room is archived".into());
            }
            let mut target_participant_id = target_participant_id.clone();
            let mut target_participant_ids = vec![];
            if sender == "Human" && participant_id.is_none() {
                if let Some(ids) = super::mentions::recipients(body, &room.participants) {
                    if ids.len() == 1 {
                        target_participant_id = ids.first().cloned();
                    } else {
                        target_participant_id = None;
                        target_participant_ids = ids;
                    }
                }
            }
            if let Some(sidechat_id) = sidechat_id.as_deref() {
                let sidechat = room
                    .sidechats
                    .iter()
                    .find(|s| s.id == sidechat_id)
                    .ok_or("sidechat not found")?;
                if participant_id
                    .as_ref()
                    .is_some_and(|id| !sidechat.participant_ids.contains(id))
                {
                    return Err("sender is not a member of this sidechat".into());
                }
                if target_participant_id
                    .as_ref()
                    .is_some_and(|id| !sidechat.participant_ids.contains(id))
                {
                    return Err("target participant is not a member of this sidechat".into());
                }
                if target_participant_ids
                    .iter()
                    .any(|id| !sidechat.participant_ids.contains(id))
                {
                    return Err("A mentioned agent is not a member of this sidechat.".into());
                }
            }
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
            let message = Message {
                id: uuid::Uuid::new_v4().to_string(),
                sender: sender.into(),
                body: body.into(),
                created_at: crate::models::now_iso(),
                participant_id,
                target_participant_id,
                target_participant_ids,
                source_event_id,
                sidechat_id,
                attachments,
            };
            room.messages.push(message);
            super::runtime::wake_dormant_for_unread_human_messages(room);
            Ok(())
        })
    }

    pub fn create_sidechat(
        &self,
        room_id: &str,
        source_message_id: &str,
        title: &str,
        participant_ids: Vec<String>,
    ) -> Result<Room, String> {
        let title = title.trim();
        if title.is_empty() || title.chars().count() > 120 {
            return Err("sidechat title must contain 1–120 characters".into());
        }
        self.update(room_id, |room| {
            if room.archived {
                return Err("room is archived".into());
            }
            let source = room
                .messages
                .iter()
                .find(|m| m.id == source_message_id)
                .ok_or("source message not found in this room")?;
            let mut allowed_ids = if let Some(source_sidechat_id) = source.sidechat_id.as_deref() {
                room.sidechats
                    .iter()
                    .find(|s| s.id == source_sidechat_id)
                    .ok_or("source sidechat not found")?
                    .participant_ids
                    .clone()
            } else if source.is_directed() {
                source
                    .target_participant_ids
                    .iter()
                    .cloned()
                    .chain(source.target_participant_id.clone())
                    .chain(source.participant_id.clone())
                    .collect()
            } else {
                room.participants
                    .iter()
                    .map(|p| p.id.clone())
                    .collect::<Vec<_>>()
            };
            if source.sidechat_id.is_some() && source.is_directed() {
                allowed_ids
                    .retain(|id| source.participant_id.as_ref() == Some(id) || source.targets(id));
            }
            let ids = if participant_ids.is_empty() {
                allowed_ids.clone()
            } else {
                participant_ids.clone()
            };
            let mut unique = Vec::new();
            for id in ids {
                if !unique.contains(&id) {
                    unique.push(id);
                }
            }
            if unique.is_empty()
                || unique.iter().any(|id| {
                    !room.participants.iter().any(|p| &p.id == id) || !allowed_ids.contains(id)
                })
            {
                return Err(
                    "sidechat participants must be room members who can see the source message"
                        .into(),
                );
            }
            let id = uuid::Uuid::new_v4().to_string();
            room.sidechats.push(Sidechat {
                id,
                title: title.into(),
                source_message_id: source_message_id.into(),
                participant_ids: unique,
                created_at: crate::models::now_iso(),
            });
            for peer in room.participants.iter_mut().filter(|p| {
                room.sidechats
                    .last()
                    .is_some_and(|s| s.participant_ids.contains(&p.id))
            }) {
                if !peer.read_message_ids.contains(&source.id) {
                    peer.read_message_ids.push(source.id.clone());
                }
            }
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
                || (peer.turn_limit_reached() && peer.pending_delivery.is_none())
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
    fn edited_instructions_persist_without_overwriting_newer_room_state() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("rooms.db");
        let store = RoomStore::open(&path).unwrap();
        let room = store.create(input(temp.path())).unwrap();
        store
            .post_message(&room.id, "New room update".into())
            .unwrap();
        let saved = store
            .save_instructions(
                &room.id,
                " Follow repository rules and use bounded local subagents. ".into(),
                room.objective.clone(),
            )
            .unwrap();
        assert_eq!(saved.messages.len(), 1);
        assert_eq!(
            saved.objective,
            "Follow repository rules and use bounded local subagents."
        );
        assert!(store
            .save_instructions(&room.id, "Stale edit".into(), room.objective)
            .is_err());
        assert!(store
            .save_instructions(&room.id, " ".into(), saved.objective.clone())
            .is_err());
        drop(store);
        let store = RoomStore::open(&path).unwrap();
        assert_eq!(store.get(&room.id).unwrap().objective, saved.objective);
        store
            .update(&room.id, |r| {
                r.archived = true;
                Ok(())
            })
            .unwrap();
        assert!(store
            .save_instructions(&room.id, "Archived edit".into(), saved.objective)
            .is_err());
    }

    #[test]
    fn conversation_room_creation_is_atomic_idempotent_and_survives_reopen() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("rooms.db");
        let store = RoomStore::open(&path).unwrap();
        let origin = RoomOrigin {
            run_id: "existing-run".into(),
            provider: "codex".into(),
            session_id: "same-thread".into(),
            title: "Nemo conversation".into(),
            message_count: 100,
            context: "Original context".into(),
        };
        let peer = Participant {
            id: "original-peer".into(),
            run_id: origin.run_id.clone(),
            provider: "codex".into(),
            name: "Original Codex".into(),
            paused: true,
            model: Some("gpt-6.1-sol".into()),
            event_cursor: 999,
            message_cursor: 1,
            ..Default::default()
        };
        let message = Message {
            id: "original-message".into(),
            sender: peer.name.clone(),
            body: "Existing work".into(),
            created_at: "yesterday".into(),
            participant_id: Some(peer.id.clone()),
            target_participant_id: None,
            target_participant_ids: vec![],
            source_event_id: Some("existing-run:998".into()),
            sidechat_id: None,
            attachments: vec![],
        };
        let room = store
            .create_from_session(
                input(temp.path()),
                origin.clone(),
                peer.clone(),
                vec![message],
            )
            .unwrap();
        assert!(room.paused);
        assert_eq!(room.participants[0].run_id, "existing-run");
        let mut alias = origin.clone();
        alias.run_id = "second-import-of-same-provider-thread".into();
        let second = store
            .create_from_session(input(temp.path()), alias, peer, vec![])
            .unwrap();
        assert_eq!(room.id, second.id);
        assert_eq!(store.list().unwrap().len(), 1);
        drop(store);
        let store = RoomStore::open(&path).unwrap();
        let recovered = store.get(&room.id).unwrap();
        assert_eq!(recovered.origin.unwrap().session_id, "same-thread");
        assert_eq!(recovered.participants[0].event_cursor, 999);
        assert_eq!(recovered.messages[0].body, "Existing work");
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

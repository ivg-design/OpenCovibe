//! Reassociate saved local chats without replacing their provider identity or history.
use super::models::{Participant, Room};
use crate::models::{RunMeta, RunStatus};

#[derive(serde::Serialize)]
pub struct AttachableChat {
    pub run_id: String,
    pub title: String,
    pub name: String,
    pub provider: String,
    pub model: Option<String>,
    pub cwd: String,
    pub previous_participant: bool,
    pub started_at: String,
}

pub fn former_peer(room: &Room, run_id: &str) -> Option<Participant> {
    if let Some(peer) = room
        .detached_participants
        .iter()
        .find(|p| p.run_id == run_id)
    {
        return Some(peer.clone());
    }
    // Older removals predate retained detached peers. Persisted event provenance
    // maps the orphan's run back to its original participant id and message color.
    let prefix = format!("{run_id}:");
    room.messages.iter().rev().find_map(|m| {
        if !m
            .source_event_id
            .as_deref()
            .is_some_and(|s| s.starts_with(&prefix))
        {
            return None;
        }
        Some(Participant {
            id: m.participant_id.clone()?,
            name: m.sender.clone(),
            run_id: run_id.into(),
            max_turns: 0,
            ..Default::default()
        })
    })
}

pub fn validate_chat(meta: &RunMeta, former: bool) -> Result<(), String> {
    if !matches!(meta.agent.as_str(), "codex" | "claude")
        || meta.remote_host_name.is_some()
        || meta.deleted_at.is_some()
        || meta.no_session_persistence
    {
        return Err("Choose a saved local Codex or Claude chat.".into());
    }
    if matches!(meta.status, RunStatus::Running | RunStatus::Pending) {
        return Err("Finish or stop this chat's current turn before attaching it.".into());
    }
    if !former {
        super::session_seed::validate_source(meta)?;
    }
    Ok(())
}

pub fn owns_attached_chat(room: &Room, meta: &RunMeta) -> bool {
    let reference = meta.resolved_conversation_ref();
    room.participants.iter().any(|p| {
        p.run_id == meta.id
            || reference.as_ref().is_some_and(|reference| {
                crate::storage::runs::get_run(&p.run_id).is_some_and(|other| {
                    other.agent == meta.agent
                        && other.resolved_conversation_ref().as_ref() == Some(reference)
                })
            })
    })
}

pub fn owns_chat(room: &Room, meta: &RunMeta) -> bool {
    let reference = meta.resolved_conversation_ref();
    owns_attached_chat(room, meta)
        || room.origin.as_ref().is_some_and(|origin| {
            if origin.run_id == meta.id {
                return true;
            }
            reference.as_ref().is_some_and(|reference| {
                let id = match reference {
                    crate::models::ConversationRef::CodexThread(id)
                    | crate::models::ConversationRef::ClaudeSession(id) => id,
                };
                origin.provider == meta.agent && &origin.session_id == id
            })
        })
}

pub fn restored_peer(
    room: &Room,
    meta: &RunMeta,
    name: &str,
    seq: u64,
    offset: u64,
) -> Result<Participant, String> {
    let former = former_peer(room, &meta.id);
    validate_chat(meta, former.is_some())?;
    if name.trim().is_empty() || name.trim().chars().count() > 80 {
        return Err("Enter an agent name with 1–80 characters.".into());
    }
    let mut peer = former.unwrap_or_else(|| Participant {
        id: uuid::Uuid::new_v4().to_string(),
        run_id: meta.id.clone(),
        max_turns: 0,
        ..Default::default()
    });
    peer.name = name.trim().into();
    if owns_attached_chat(room, meta)
        || room
            .participants
            .iter()
            .any(|p| p.id == peer.id || p.name.eq_ignore_ascii_case(&peer.name))
    {
        return Err(
            "This chat or agent name already belongs to the room. Choose another name or chat."
                .into(),
        );
    }
    peer.provider = meta.agent.clone();
    if peer.model.is_none() {
        peer.model = meta.model.clone();
    }
    peer.paused = true;
    peer.state = "paused".into();
    peer.pending_delivery = None;
    peer.last_error = None;
    peer.no_progress_turns = 0;
    peer.work_signature = None;
    peer.event_cursor = seq;
    peer.event_offset = Some(offset);
    peer.message_cursor = room.messages.len();
    peer.unread_message_ids.clear();
    Ok(peer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rooms::{models::CreateRoomInput, store::RoomStore};

    #[test]
    fn reassociation_restores_identity_without_replaying_history_or_starting_work() {
        let temp = tempfile::tempdir().unwrap();
        let store = RoomStore::open(&temp.path().join("rooms.db")).unwrap();
        let mut room = store
            .create(CreateRoomInput {
                title: "Room".into(),
                objective: "Continue".into(),
                repo_path: temp.path().to_string_lossy().into(),
                repository: "acme/test".into(),
                create_project: false,
            })
            .unwrap();
        let meta: RunMeta = serde_json::from_value(serde_json::json!({
            "id":"saved-run", "prompt":"Original work", "cwd":temp.path(), "agent":"codex", "status":"stopped",
            "started_at":"2026-10-02", "session_id":"original-thread", "model":"original-model"
        })).unwrap();
        let old = Participant {
            id: "old-peer".into(),
            name: "Original name".into(),
            run_id: meta.id.clone(),
            model: Some("saved-model".into()),
            effort: Some("high".into()),
            branch: Some("room/old".into()),
            ..Default::default()
        };
        room.detached_participants.push(old.clone());
        let restored = restored_peer(&room, &meta, "Renamed", 200, 500).unwrap();
        assert_eq!(restored.id, old.id);
        assert_eq!(restored.run_id, meta.id);
        assert_eq!(restored.effort, old.effort);
        assert_eq!(restored.branch, old.branch);
        assert_eq!(restored.model, old.model);
        assert!(restored.paused);
        assert_eq!(restored.event_offset, Some(500));
        assert_eq!(restored.event_cursor, 200);
        assert!(restored.pending_delivery.is_none());
        let saved = store
            .attach_chat(&room.id, restored.clone(), &meta)
            .unwrap();
        assert_eq!(saved.participants.len(), 1);
        assert_eq!(
            store
                .attach_chat(&room.id, restored.clone(), &meta)
                .unwrap()
                .participants
                .len(),
            1
        );
        let other = store
            .create(CreateRoomInput {
                title: "Other".into(),
                objective: "Work".into(),
                repo_path: meta.cwd.clone(),
                repository: "acme/test".into(),
                create_project: false,
            })
            .unwrap();
        assert!(store.attach_chat(&other.id, restored, &meta).is_err());
        assert!(restored_peer(&room, &meta, " ", 200, 500).is_err());
        let mut running = meta.clone();
        running.status = RunStatus::Running;
        assert!(restored_peer(&room, &running, "Agent", 200, 500).is_err());
        let mut conflicting = meta.clone();
        conflicting.id = "other-run".into();
        let conflicting_peer = Participant {
            id: "other-peer".into(),
            run_id: conflicting.id.clone(),
            name: "renamed".into(),
            ..Default::default()
        };
        assert!(store
            .attach_chat(&room.id, conflicting_peer, &conflicting)
            .is_err());
        store
            .update(&other.id, |r| {
                r.archived = true;
                Ok(())
            })
            .unwrap();
        assert!(store
            .attach_chat(
                &other.id,
                Participant {
                    id: "third".into(),
                    run_id: conflicting.id.clone(),
                    name: "Unique".into(),
                    ..Default::default()
                },
                &conflicting
            )
            .is_err());
        // Recover peers removed by old versions from message provenance, keeping
        // references from side chats, requests and message colors on the same id.
        room.detached_participants.clear();
        room.messages.push(
            serde_json::from_value(serde_json::json!({
                "id":"message", "sender":"Legacy agent", "body":"Work report", "created_at":"today",
                "participant_id":"legacy-peer", "source_event_id":"saved-run:199"
            }))
            .unwrap(),
        );
        let recovered = restored_peer(&room, &meta, "Legacy agent", 200, 500).unwrap();
        assert_eq!(recovered.id, "legacy-peer");
        assert_eq!(recovered.model, meta.model);
        assert_eq!(recovered.message_cursor, room.messages.len());
        drop(store);
        let reopened = RoomStore::open(&temp.path().join("rooms.db")).unwrap();
        assert_eq!(
            reopened.get(&room.id).unwrap().participants[0].id,
            "old-peer"
        );
    }
}

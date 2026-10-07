//! Explicit owner-granted room control; never bypass provider waits or replay a start.
use super::{decode, load_room, Message, Principal, Receipt, Room, RoomStore, Send};
use crate::rooms::models::Participant;
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResumeRoom {
    pub room_id: String,
    pub conversation_ref: String,
    pub client_action_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WakeAgent {
    pub agent_id: String,
    pub conversation_ref: String,
    pub client_action_id: String,
    pub text: String,
    #[serde(default)]
    pub reset_turn_budget: bool,
}

fn action_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 160 {
        Err("client_action_id must contain 1–160 bytes".into())
    } else {
        Ok(())
    }
}

pub(super) fn allowed_actions(
    principal: &Principal,
    room: &Room,
    peer: &Participant,
) -> Vec<&'static str> {
    let mut actions = vec!["read"];
    if room.archived {
        return actions;
    }
    if principal.scope("send").is_ok() {
        actions.push("queue");
    }
    if principal.scope("control").is_ok() {
        actions.push("resume_room");
        if principal.scope("send").is_ok() && resume_peer(&mut peer.clone(), false).is_ok() {
            actions.push("wake_agent");
        }
    }
    actions
}

pub(super) fn resume_peer(peer: &mut Participant, reset_budget: bool) -> Result<(), String> {
    if matches!(peer.state.as_str(), "waiting" | "quota") {
        return Err(
            "Agent awaits permission, recovery or quota; resolve it in OpenCovibe first".into(),
        );
    }
    if peer.pending_delivery.is_some() || matches!(peer.state.as_str(), "busy" | "running") {
        if peer.paused || reset_budget {
            return Err(
                "Agent has an unsettled turn; settle it in OpenCovibe before resuming".into(),
            );
        }
        // Already running: keep ownership and the existing turn; queue the new message.
        return Ok(());
    }
    if peer.turn_limit_reached() && !reset_budget {
        return Err(
            "Agent turn limit reached; use reset_turn_budget=true to grant another budget".into(),
        );
    }
    if !matches!(
        peer.state.as_str(),
        "idle" | "paused" | "blocked" | "no_progress" | "completed" | "failed" | "budget_exhausted"
    ) {
        return Err("Agent state requires recovery in OpenCovibe".into());
    }
    peer.paused = false;
    peer.state = "idle".into();
    peer.last_error = None;
    peer.no_progress_turns = 0;
    peer.work_signature = None;
    if reset_budget {
        peer.wake_count = 0;
    }
    Ok(())
}

impl RoomStore {
    pub(super) fn bridge_resume_room(
        &self,
        principal: &Principal,
        input: ResumeRoom,
    ) -> Result<Value, String> {
        principal.scope("control")?;
        principal.binding(&input.room_id, &input.conversation_ref)?;
        action_id(&input.client_action_id)?;
        let payload = json!({"action":"resume_room","room_id":input.room_id,"conversation_ref":input.conversation_ref}).to_string();
        let mut conn = self.connection.lock().map_err(|e| e.to_string())?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        let old: Option<(String, String)> = tx
            .query_row(
                "SELECT payload,result FROM bridge_controls WHERE principal=?1 AND client_id=?2",
                params![principal.id, input.client_action_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some((saved, result)) = old {
            if saved != payload {
                return Err("Idempotency conflict: client_action_id payload changed".into());
            }
            let mut result: Value = decode(result)?;
            result["duplicate"] = json!(true);
            result["room_paused"] = json!(load_room(&tx, &input.room_id)?.paused);
            return Ok(result);
        }
        let mut room = load_room(&tx, &input.room_id)?;
        if room.archived {
            return Err("Archived room cannot be resumed".into());
        }
        let now = crate::models::now_iso();
        let changed = room.paused;
        room.paused = false;
        // A room pause stops peers, but does not grant a new individual budget or
        // clear permission/quota waits. Independently paused peers stay paused.
        for peer in &mut room.participants {
            if !peer.paused && peer.state == "paused" && peer.pending_delivery.is_none() {
                peer.state = "idle".into();
            }
        }
        if changed {
            room.messages.push(Message {
                id: uuid::Uuid::new_v4().to_string(),
                sender: "Dotcliffe".into(),
                body: "Resumed the room. Individually paused agents remain paused.".into(),
                created_at: now.clone(),
                // Audit entry is visible context, not a new broadcast work instruction.
                participant_id: Some(format!("external-controller:{}", principal.id)),
                target_participant_id: None,
                target_participant_ids: vec![],
                source_event_id: Some(format!("bridge-control:{}", uuid::Uuid::new_v4())),
                sidechat_id: None,
                attachments: vec![],
            });
        }
        room.updated_at = now.clone();
        let result = json!({"action":"resume_room","room_id":room.id,"room_paused":false,"changed":changed,"duplicate":false,"applied_at":now,"individually_paused_agents":room.participants.iter().filter(|p|p.paused).map(|p|format!("{}/{}",room.id,p.id)).collect::<Vec<_>>()});
        tx.execute(
            "UPDATE rooms SET payload=?1 WHERE id=?2",
            params![
                serde_json::to_string(&room).map_err(|e| e.to_string())?,
                room.id
            ],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO bridge_controls VALUES (?1,?2,?3,?4)",
            params![
                principal.id,
                input.client_action_id,
                payload,
                result.to_string()
            ],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(result)
    }

    pub(super) fn bridge_wake_agent(
        &self,
        principal: &Principal,
        input: WakeAgent,
    ) -> Result<Receipt, String> {
        principal.scope("control")?;
        action_id(&input.client_action_id)?;
        let send = Send {
            agent_id: input.agent_id,
            conversation_ref: input.conversation_ref,
            client_message_id: format!("wake:{}", input.client_action_id),
            text: input.text,
            reply_to_message_id: None,
            mode: "queue".into(),
            attachments: vec![],
        };
        self.bridge_send_mode(principal, &send, true, input.reset_turn_budget)
    }
}

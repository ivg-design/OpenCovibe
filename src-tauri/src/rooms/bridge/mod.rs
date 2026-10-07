//! Private, scoped external messaging. This module never owns a provider process.
mod control;
mod direct;
mod events;
mod protocol;
pub(crate) mod relay;
#[cfg(test)]
mod tests;

pub use protocol::handler;
pub fn start_events(store: std::sync::Arc<RoomStore>, cancel: tokio_util::sync::CancellationToken) {
    events::start(store, cancel);
}
pub fn start_direct(
    store: std::sync::Arc<RoomStore>,
    sessions: crate::agent::adapter::ActorSessionMap,
    cancel: tokio_util::sync::CancellationToken,
) {
    direct::start(store, sessions, cancel);
}
use super::{
    models::{Message, Room},
    store::RoomStore,
};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Principal {
    pub id: String,
    pub token_sha256: String,
    pub bindings: Vec<Binding>,
    #[serde(default)]
    pub all_rooms: bool,
    #[serde(default)]
    pub all_sessions: bool,
    #[serde(default)]
    pub conversations: Vec<String>,
    pub callback_hosts: Vec<String>,
    pub scopes: Vec<String>,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub room_id: String,
    pub conversation_ref: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Access {
    principals: Vec<Principal>,
}

pub fn access_path() -> std::path::PathBuf {
    crate::storage::data_dir().join("bridge-access.json")
}
fn read_access(path: &std::path::Path) -> Result<Access, String> {
    let metadata = std::fs::metadata(path).map_err(|_| "External messaging is not configured")?;
    if !metadata.is_file() || metadata.len() > 128 * 1024 {
        return Err("Invalid bridge access file".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.mode() & 0o077 != 0 || metadata.uid() != unsafe { libc::geteuid() } {
            return Err("Bridge access file must be private to its owner".into());
        }
    }
    let access: Access =
        serde_json::from_slice(&std::fs::read(path).map_err(|_| "Cannot read bridge access file")?)
            .map_err(|_| "Invalid bridge access configuration")?;
    let mut ids = HashSet::new();
    let mut hashes = HashSet::new();
    for principal in &access.principals {
        if principal.id.is_empty()
            || !ids.insert(&principal.id)
            || principal.token_sha256.len() != 64
            || !principal
                .token_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || !hashes.insert(&principal.token_sha256)
            || principal
                .bindings
                .iter()
                .any(|b| b.room_id.is_empty() || b.conversation_ref.is_empty())
            || principal.conversations.iter().any(String::is_empty)
            || ((principal.all_rooms || principal.all_sessions)
                && principal.conversations.is_empty())
        {
            return Err("Invalid or duplicate bridge principal".into());
        }
    }
    Ok(access)
}
pub(super) fn current_principal(id: &str) -> Result<Principal, String> {
    read_access(&access_path())?
        .principals
        .into_iter()
        .find(|p| p.id == id)
        .ok_or("Principal revoked".into())
}
fn authenticate_access(access: Access, token: &str) -> Result<Principal, String> {
    if token.len() < 32 || token.len() > 4096 {
        return Err("Unauthorized".into());
    }
    let hash = format!("{:x}", Sha256::digest(token.as_bytes()));
    access
        .principals
        .into_iter()
        .find(|p| constant_eq(hash.as_bytes(), p.token_sha256.as_bytes()))
        .ok_or("Unauthorized".into())
}
pub fn authenticate(token: &str) -> Result<Principal, String> {
    authenticate_access(read_access(&access_path())?, token)
}
fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |v, (a, b)| v | (a ^ b)) == 0
}
impl Principal {
    fn binding(&self, room: &str, conversation: &str) -> Result<(), String> {
        if self
            .bindings
            .iter()
            .any(|b| b.room_id == room && b.conversation_ref == conversation)
            || (self.all_rooms && self.conversations.iter().any(|c| c == conversation))
        {
            Ok(())
        } else {
            Err("Forbidden room/conversation binding".into())
        }
    }
    fn scope(&self, scope: &str) -> Result<(), String> {
        if self.scopes.iter().any(|s| s == scope) {
            Ok(())
        } else {
            Err("Forbidden scope".into())
        }
    }
    fn room(&self, room: &str) -> Result<(), String> {
        if self.all_rooms || self.bindings.iter().any(|b| b.room_id == room) {
            Ok(())
        } else {
            Err("Forbidden room".into())
        }
    }
    fn conversation(&self, conversation: &str) -> Result<(), String> {
        if self
            .bindings
            .iter()
            .any(|b| b.conversation_ref == conversation)
            || ((self.all_rooms || self.all_sessions)
                && self.conversations.iter().any(|c| c == conversation))
        {
            Ok(())
        } else {
            Err("Forbidden conversation".into())
        }
    }
    fn session(&self, conversation: &str) -> Result<(), String> {
        if self.all_sessions && self.conversations.iter().any(|c| c == conversation) {
            Ok(())
        } else {
            Err("Forbidden session/conversation binding".into())
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Send {
    pub agent_id: String,
    pub text: String,
    pub client_message_id: String,
    pub conversation_ref: String,
    #[serde(default)]
    pub reply_to_message_id: Option<String>,
    #[serde(default = "queue_mode")]
    pub mode: String,
    #[serde(default)]
    pub attachments: Vec<String>,
}
fn queue_mode() -> String {
    "queue".into()
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub message_id: String,
    pub room_id: String,
    #[serde(default = "room_target")]
    pub target_kind: String,
    #[serde(default)]
    pub target_run_id: Option<String>,
    pub agent_id: String,
    pub conversation_ref: String,
    pub room_message_id: String,
    pub state: String,
    pub queued_at: String,
    pub delivered_at: Option<String>,
    pub running_at: Option<String>,
    pub completed_at: Option<String>,
    pub delivery_id: Option<String>,
    pub provider_turn_id: Option<String>,
    pub run_id: Option<String>,
    pub error: Option<String>,
    pub blocked_reason: Option<String>,
}
fn room_target() -> String {
    "room".into()
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Reply {
    pub event_id: String,
    pub message_id: String,
    pub reply_to_message_id: String,
    pub conversation_ref: String,
    pub agent_id: String,
    pub sequence: i64,
    pub timestamp: String,
    pub status: String,
    pub text: String,
    pub room_message_id: String,
}

pub(super) fn schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS bridge_receipts (
      id TEXT PRIMARY KEY, principal TEXT NOT NULL, client_id TEXT NOT NULL,
      payload TEXT NOT NULL, room_id TEXT NOT NULL, receipt TEXT NOT NULL,
      UNIQUE(principal,client_id));
      CREATE INDEX IF NOT EXISTS bridge_receipts_room ON bridge_receipts(room_id);
      CREATE TABLE IF NOT EXISTS bridge_outbox (
      seq INTEGER PRIMARY KEY AUTOINCREMENT, principal TEXT NOT NULL,
      conversation TEXT NOT NULL, event_key TEXT NOT NULL UNIQUE, payload TEXT NOT NULL);
      CREATE INDEX IF NOT EXISTS bridge_outbox_cursor ON bridge_outbox(principal,conversation,seq);
      CREATE TABLE IF NOT EXISTS bridge_direct (
      receipt_id TEXT PRIMARY KEY, run_id TEXT NOT NULL, phase TEXT NOT NULL,
      cursor INTEGER NOT NULL DEFAULT 0, offset INTEGER NOT NULL DEFAULT 0);
      CREATE INDEX IF NOT EXISTS bridge_direct_run ON bridge_direct(run_id,phase);
      CREATE TABLE IF NOT EXISTS bridge_controls (
      principal TEXT NOT NULL, client_id TEXT NOT NULL, payload TEXT NOT NULL,
      result TEXT NOT NULL, PRIMARY KEY(principal,client_id));
      CREATE TABLE IF NOT EXISTS bridge_subscriptions (
      id TEXT PRIMARY KEY, principal TEXT NOT NULL, payload TEXT NOT NULL);",
    )
    .map_err(|e| e.to_string())
}
fn load_room(conn: &Connection, id: &str) -> Result<Room, String> {
    let raw: String = conn
        .query_row("SELECT payload FROM rooms WHERE id=?1", [id], |r| r.get(0))
        .map_err(|_| "Room unavailable")?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}
fn decode<T: serde::de::DeserializeOwned>(s: String) -> Result<T, String> {
    serde_json::from_str(&s).map_err(|e| e.to_string())
}
fn save_receipt(conn: &Connection, r: &Receipt) -> Result<(), String> {
    conn.execute(
        "UPDATE bridge_receipts SET receipt=?1 WHERE id=?2",
        params![
            serde_json::to_string(r).map_err(|e| e.to_string())?,
            r.message_id
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
fn terminal(state: &str) -> bool {
    matches!(state, "completed" | "failed" | "cancelled" | "ambiguous")
}
fn blocked(room: &Room, peer: &super::models::Participant) -> Option<String> {
    if room.archived {
        Some("Room archived".into())
    } else if room.paused {
        Some("Room paused".into())
    } else if peer.paused {
        Some(format!("Participant {}", peer.state))
    } else if peer.turn_limit_reached() {
        Some("Turn budget exhausted".into())
    } else if !matches!(peer.state.as_str(), "idle" | "busy" | "running") {
        Some(format!("Participant {}", peer.state))
    } else {
        None
    }
}
impl RoomStore {
    pub fn bridge_agents(
        &self,
        principal: &Principal,
        room_id: Option<&str>,
    ) -> Result<serde_json::Value, String> {
        principal.scope("read")?;
        if let Some(id) = room_id {
            principal.room(id)?;
        }
        let rooms = self.list()?;
        let mut agents: Vec<_> = rooms.iter().filter(|r| principal.room(&r.id).is_ok() && room_id.is_none_or(|id| id == r.id)).flat_map(|r| r.participants.iter().map(move |p| {
            let meta = crate::storage::runs::get_run(&p.run_id);
            serde_json::json!({"agent_id":format!("{}/{}",r.id,p.id),"room_id":r.id,"participant_id":p.id,"name":p.name,"provider":p.provider,"provider_thread_id":meta.as_ref().and_then(|m| m.resolved_conversation_ref()).map(|reference| match reference { crate::models::ConversationRef::ClaudeSession(id) | crate::models::ConversationRef::CodexThread(id) => id }),"run_id":p.run_id,"state":p.state,"room_paused":r.paused,"participant_paused":p.paused,"blocked_reason":blocked(r,p),"turn_limit_reached":p.turn_limit_reached(),"wake_count":p.wake_count,"max_turns":p.max_turns,"allowed_actions":control::allowed_actions(principal,r,p)})
        })).collect();
        if room_id.is_none() {
            agents.extend(direct::agents(principal, &rooms));
        }
        Ok(serde_json::json!({"agents":agents}))
    }
    pub fn bridge_send(&self, principal: &Principal, input: &Send) -> Result<Receipt, String> {
        self.bridge_send_mode(principal, input, false, false)
    }
    pub(super) fn bridge_send_mode(
        &self,
        principal: &Principal,
        input: &Send,
        wake: bool,
        reset_turn_budget: bool,
    ) -> Result<Receipt, String> {
        principal.scope("send")?;
        if wake {
            principal.scope("control")?;
        }
        principal.conversation(&input.conversation_ref)?;
        if let Some(run_id) = input.agent_id.strip_prefix("session/") {
            if wake {
                return Err("Wake controls apply to room participants only".into());
            }
            return direct::send(self, principal, input, run_id);
        }
        let (room_id, peer_id) = input.agent_id.split_once('/').ok_or("Invalid agent_id")?;
        principal.binding(room_id, &input.conversation_ref)?;
        if input.mode != "queue" {
            return Err(
                "Steering is not exposed: use queue through the owning room scheduler".into(),
            );
        }
        if input.text.trim().is_empty()
            || input.text.len() > 32_000
            || input.client_message_id.is_empty()
            || input.client_message_id.len() > 200
        {
            return Err("Invalid message text or client_message_id".into());
        }
        let payload = if wake {
            serde_json::json!({"action":"wake_agent","reset_turn_budget":reset_turn_budget,"message":input}).to_string()
        } else {
            serde_json::to_string(input).map_err(|e| e.to_string())?
        };
        // A durable retry must work even if an attachment has since been removed.
        {
            let conn = self.connection.lock().map_err(|e| e.to_string())?;
            let existing: Option<(String, String)> = conn.query_row("SELECT payload,receipt FROM bridge_receipts WHERE principal=?1 AND client_id=?2", params![principal.id,input.client_message_id], |r| Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e| e.to_string())?;
            if let Some((saved, receipt)) = existing {
                return if saved == payload {
                    decode(receipt)
                } else {
                    Err("Idempotency conflict: client_message_id payload changed".into())
                };
            }
        }
        let files = self.resolve_attachments(room_id, &input.attachments)?;
        let mut conn = self.connection.lock().map_err(|e| e.to_string())?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        let existing: Option<(String, String)> = tx
            .query_row(
                "SELECT payload,receipt FROM bridge_receipts WHERE principal=?1 AND client_id=?2",
                params![principal.id, input.client_message_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some((saved, receipt)) = existing {
            return if saved == payload {
                decode(receipt)
            } else {
                Err("Idempotency conflict: client_message_id payload changed".into())
            };
        }
        if let Some(reply) = &input.reply_to_message_id {
            let old: Receipt = authorized_receipt(&tx, principal, reply)?;
            if old.conversation_ref != input.conversation_ref || old.room_id != room_id {
                return Err("Reply room/conversation mismatch".into());
            }
        }
        let mut room = load_room(&tx, room_id)?;
        if room.archived {
            return Err("Room archived".into());
        }
        if wake {
            let peer = room
                .participants
                .iter_mut()
                .find(|p| p.id == peer_id)
                .ok_or("Participant unavailable")?;
            control::resume_peer(peer, reset_turn_budget)?;
        }
        let peer = room
            .participants
            .iter()
            .find(|p| p.id == peer_id)
            .ok_or("Participant unavailable")?;
        let id = uuid::Uuid::new_v4().to_string();
        let room_message_id = uuid::Uuid::new_v4().to_string();
        let now = crate::models::now_iso();
        let receipt = Receipt {
            message_id: id.clone(),
            room_id: room_id.into(),
            target_kind: room_target(),
            target_run_id: None,
            agent_id: input.agent_id.clone(),
            conversation_ref: input.conversation_ref.clone(),
            room_message_id: room_message_id.clone(),
            state: "queued".into(),
            queued_at: now.clone(),
            delivered_at: None,
            running_at: None,
            completed_at: None,
            delivery_id: None,
            provider_turn_id: None,
            run_id: Some(peer.run_id.clone()),
            error: None,
            blocked_reason: blocked(&room, peer),
        };
        room.messages.push(Message {
            id: room_message_id,
            sender: if wake {
                "Dotcliffe · resumed agent"
            } else {
                "External message"
            }
            .into(),
            body: input.text.clone(),
            created_at: now.clone(),
            participant_id: None,
            target_participant_id: Some(peer_id.into()),
            target_participant_ids: vec![],
            source_event_id: Some(if wake {
                format!("bridge:wake:{id}")
            } else {
                format!("bridge:{id}")
            }),
            sidechat_id: None,
            attachments: files,
        });
        room.updated_at = now;
        tx.execute(
            "INSERT INTO bridge_receipts VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                id,
                principal.id,
                input.client_message_id,
                payload,
                room_id,
                serde_json::to_string(&receipt).map_err(|e| e.to_string())?
            ],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE rooms SET payload=?1 WHERE id=?2",
            params![
                serde_json::to_string(&room).map_err(|e| e.to_string())?,
                room_id
            ],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(receipt)
    }
    pub fn bridge_get(&self, principal: &Principal, id: &str) -> Result<Receipt, String> {
        principal.scope("read")?;
        let conn = self.connection.lock().map_err(|e| e.to_string())?;
        authorized_receipt(&conn, principal, id)
    }
    pub fn bridge_replies(
        &self,
        principal: &Principal,
        conversation: &str,
        cursor: i64,
        limit: usize,
    ) -> Result<serde_json::Value, String> {
        principal.scope("read")?;
        principal.conversation(conversation)?;
        if cursor < 0 || !(1..=100).contains(&limit) {
            return Err("Invalid cursor or limit".into());
        }
        let conn = self.connection.lock().map_err(|e| e.to_string())?;
        let mut query=conn.prepare("SELECT seq,payload FROM bridge_outbox WHERE principal=?1 AND conversation=?2 AND seq>?3 ORDER BY seq LIMIT ?4").map_err(|e|e.to_string())?;
        let rows = query
            .query_map(
                params![principal.id, conversation, cursor, limit as i64],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
            )
            .map_err(|e| e.to_string())?;
        let mut replies = Vec::new();
        let mut next = cursor;
        for row in rows {
            let (seq, payload) = row.map_err(|e| e.to_string())?;
            let reply: Reply = decode(payload)?;
            let receipt = authorized_receipt(&conn, principal, &reply.reply_to_message_id)?;
            if receipt.target_kind == "session" {
                principal.session(&receipt.conversation_ref)?;
            } else {
                principal.room(&receipt.room_id)?;
            }
            replies.push(reply);
            next = seq;
        }
        Ok(serde_json::json!({"replies":replies,"next_cursor":next}))
    }
}
fn authorized_receipt(
    conn: &Connection,
    principal: &Principal,
    id: &str,
) -> Result<Receipt, String> {
    let raw: String = conn
        .query_row(
            "SELECT receipt FROM bridge_receipts WHERE id=?1 AND principal=?2",
            params![id, principal.id],
            |r| r.get(0),
        )
        .map_err(|_| "Message unavailable")?;
    let r: Receipt = decode(raw)?;
    if r.target_kind == "session" {
        principal.session(&r.conversation_ref)?;
    } else {
        principal.binding(&r.room_id, &r.conversation_ref)?;
    }
    Ok(r)
}
pub(super) struct Snapshot {
    participants: Vec<super::models::Participant>,
    message_ids: HashSet<String>,
}
pub(super) fn snapshot(conn: &Connection, room: &Room) -> Result<Option<Snapshot>, String> {
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM bridge_receipts WHERE room_id=?1 AND json_extract(receipt,'$.state') NOT IN ('completed','failed','cancelled','ambiguous'))",
            [&room.id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(exists.then(|| Snapshot {
        participants: room.participants.clone(),
        message_ids: room.messages.iter().map(|m| m.id.clone()).collect(),
    }))
}
pub(super) fn reconcile(
    conn: &Connection,
    before: &Snapshot,
    room: &Room,
    completion_event: bool,
) -> Result<(), String> {
    let mut query = conn
        .prepare("SELECT principal,receipt FROM bridge_receipts WHERE room_id=?1 AND json_extract(receipt,'$.state') NOT IN ('completed','failed','cancelled','ambiguous')")
        .map_err(|e| e.to_string())?;
    let rows = query
        .query_map([&room.id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    for (principal, raw) in rows {
        let mut receipt: Receipt = decode(raw)?;
        if terminal(&receipt.state) {
            continue;
        }
        let peer_id = receipt
            .agent_id
            .split_once('/')
            .map(|(_, p)| p)
            .ok_or("Invalid stored agent")?;
        let peer = room.participants.iter().find(|p| p.id == peer_id);
        let old = before.participants.iter().find(|p| p.id == peer_id);
        let now = crate::models::now_iso();
        if let Some(p) = peer {
            receipt.blocked_reason = blocked(room, p);
            let delivery = p
                .pending_delivery
                .as_ref()
                .filter(|d| d.message_id.as_deref() == Some(&receipt.room_message_id));
            if let Some(d) = delivery {
                receipt.provider_turn_id = d.provider_turn_id.clone();
                receipt.delivery_id = Some(d.id.clone());
                receipt.run_id = Some(p.run_id.clone());
                receipt.state = if p.state == "waiting" {
                    "awaiting_input"
                } else if d.turn_started {
                    "running"
                } else if d.state == "sent" {
                    "delivered"
                } else {
                    "queued"
                }
                .into();
                if matches!(p.state.as_str(), "failed" | "quota") {
                    receipt.state = "failed".into();
                    receipt.error = p.last_error.clone();
                    receipt.completed_at = Some(now.clone());
                }
                if d.state == "sent" {
                    receipt.delivered_at.get_or_insert(now.clone());
                }
                if d.turn_started {
                    receipt.running_at.get_or_insert(now.clone());
                }
            }
            if let Some(d) = old
                .and_then(|p| p.pending_delivery.as_ref())
                .filter(|d| d.message_id.as_deref() == Some(&receipt.room_message_id))
            {
                for message in room.messages.iter().filter(|m| {
                    !before.message_ids.contains(m.id.as_str())
                        && m.participant_id.as_deref() == Some(peer_id)
                        && m.sidechat_id == d.sidechat_id
                }) {
                    // Only owner-projected visible messages reach this outbox. No raw bus events.
                    let key = format!("{}:{}", receipt.message_id, message.id);
                    let reply = Reply {
                        event_id: format!("evt-{key}"),
                        message_id: message.id.clone(),
                        reply_to_message_id: receipt.message_id.clone(),
                        conversation_ref: receipt.conversation_ref.clone(),
                        agent_id: receipt.agent_id.clone(),
                        sequence: 0,
                        timestamp: message.created_at.clone(),
                        status: "visible_reply".into(),
                        text: message.body.clone(),
                        room_message_id: message.id.clone(),
                    };
                    conn.execute("INSERT OR IGNORE INTO bridge_outbox (principal,conversation,event_key,payload) VALUES (?1,?2,?3,?4)",params![principal,receipt.conversation_ref,key,serde_json::to_string(&reply).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
                    let seq: i64 = conn
                        .query_row(
                            "SELECT seq FROM bridge_outbox WHERE event_key=?1",
                            [&key],
                            |r| r.get(0),
                        )
                        .map_err(|e| e.to_string())?;
                    let mut reply = reply;
                    reply.sequence = seq;
                    conn.execute(
                        "UPDATE bridge_outbox SET payload=?1 WHERE seq=?2",
                        params![
                            serde_json::to_string(&reply).map_err(|e| e.to_string())?,
                            seq
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                }
                if p.pending_delivery.is_none() {
                    receipt.state = if p.state == "failed" || p.state == "quota" {
                        "failed"
                    } else if completion_event {
                        "completed"
                    } else if d.turn_started {
                        // Once the provider turn starts, stopping it does not establish
                        // whether the requested work or side effects already happened.
                        "ambiguous"
                    } else if p.state == "paused" || p.state == "waiting" {
                        "cancelled"
                    } else {
                        "ambiguous"
                    }
                    .into();
                    receipt.completed_at = Some(now);
                    receipt.error = p.last_error.clone();
                }
            }
        } else {
            receipt.state = "cancelled".into();
            receipt.error = Some("Participant removed".into());
            receipt.completed_at = Some(now);
        }
        if room.archived && receipt.delivery_id.is_none() {
            receipt.state = "cancelled".into();
            receipt.error = Some("Room archived before delivery".into());
        }
        save_receipt(conn, &receipt)?;
    }
    Ok(())
}

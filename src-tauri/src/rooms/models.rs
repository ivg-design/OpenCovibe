use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomProject {
    pub id: String,
    pub number: u64,
    pub title: String,
    pub url: String,
    pub owner: String,
    pub repository: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoardItem {
    pub id: String,
    pub title: String,
    pub url: Option<String>,
    pub status: String,
    pub priority: Option<String>,
    pub agent: Option<String>,
    pub kind: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub number: Option<u64>,
    #[serde(default)]
    pub assignees: Vec<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub linked_prs: Vec<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Board {
    pub items: Vec<BoardItem>,
    pub synced_at: Option<String>,
    pub error: Option<String>,
    pub total_count: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Participant {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub run_id: String,
    pub paused: bool,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub brief: Option<String>,
    #[serde(default)]
    pub worktree_path: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default = "idle_state")]
    pub state: String,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub last_wake_at: Option<i64>,
    #[serde(default)]
    pub wake_count: u32,
    #[serde(default = "turn_limit")]
    pub max_turns: u32,
    #[serde(default)]
    pub event_cursor: u64,
    #[serde(default)]
    pub event_offset: Option<u64>,
    #[serde(default)]
    pub message_cursor: usize,
    #[serde(default)]
    pub pending_delivery: Option<Delivery>,
    #[serde(default)]
    pub active_sidechat_id: Option<String>,
    #[serde(default)]
    pub read_message_ids: Vec<String>,
    #[serde(default)]
    pub unread_message_ids: Vec<String>,
    #[serde(default)]
    pub no_progress_turns: u32,
    #[serde(default)]
    pub work_signature: Option<String>,
}

impl Participant {
    /// Zero disables the optional room-wide count of this participant's starts.
    pub fn turn_limit_reached(&self) -> bool {
        self.max_turns > 0 && self.wake_count >= self.max_turns
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParticipantSettings {
    pub name: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub max_turns: u32,
}

impl ParticipantSettings {
    pub fn same_execution_settings(&self, other: &Self) -> bool {
        self.model == other.model
            && self.effort == other.effort
            && self.max_turns == other.max_turns
    }
}

impl Participant {
    pub fn settings(&self) -> ParticipantSettings {
        ParticipantSettings {
            name: self.name.clone(),
            model: self.model.clone(),
            effort: self.effort.clone(),
            max_turns: self.max_turns,
        }
    }
}

fn idle_state() -> String {
    "idle".into()
}
pub fn turn_limit() -> u32 {
    20
}
fn yes() -> bool {
    true
}
pub fn default_max_concurrent() -> u32 {
    3
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delivery {
    pub id: String,
    pub reason: String,
    pub text: String,
    pub created_at: i64,
    pub state: String,
    pub task_id: Option<String>,
    pub timer_id: Option<String>,
    #[serde(default)]
    pub sidechat_id: Option<String>,
    #[serde(default)]
    pub message_id: Option<String>,
    #[serde(default)]
    pub attachment_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claim {
    pub task_id: String,
    pub participant_id: String,
    pub state: String,
    pub updated_at: String,
    pub summary: Option<String>,
    pub evidence: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomAttachment {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub sender: String,
    pub body: String,
    pub created_at: String,
    #[serde(default)]
    pub participant_id: Option<String>,
    #[serde(default)]
    pub target_participant_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_participant_ids: Vec<String>,
    #[serde(default)]
    pub source_event_id: Option<String>,
    #[serde(default)]
    pub sidechat_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<RoomAttachment>,
}

impl Message {
    pub fn targets(&self, peer_id: &str) -> bool {
        if !self.target_participant_ids.is_empty() {
            self.target_participant_ids.iter().any(|id| id == peer_id)
        } else {
            self.target_participant_id
                .as_deref()
                .is_none_or(|id| id == peer_id)
        }
    }
    pub fn is_directed(&self) -> bool {
        self.target_participant_id.is_some() || !self.target_participant_ids.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sidechat {
    pub id: String,
    pub title: String,
    pub source_message_id: String,
    pub participant_ids: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timer {
    pub id: String,
    pub participant_id: String,
    pub message: String,
    pub interval_seconds: u64,
    pub idle_only: bool,
    pub enabled: bool,
    pub next_due_at: i64,
    #[serde(default)]
    pub queued_at: Option<i64>,
    #[serde(default)]
    pub max_deliveries: Option<u32>,
    #[serde(default)]
    pub ends_at: Option<i64>,
    pub delivered_count: u32,
    pub last_error: Option<String>,
}

impl Timer {
    /// Whether this timer has reached its configured delivery-count or date limit.
    pub fn is_exhausted_at(&self, now_ms: i64) -> bool {
        self.max_deliveries
            .is_some_and(|limit| self.delivered_count >= limit)
            || self.ends_at.is_some_and(|ends_at| now_ms >= ends_at)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStage {
    #[default]
    NotStarted,
    Creating,
    Uncertain,
    Ready,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room {
    pub id: String,
    pub title: String,
    pub objective: String,
    pub repo_path: String,
    pub repository: String,
    pub created_at: String,
    pub updated_at: String,
    pub paused: bool,
    pub project: Option<RoomProject>,
    #[serde(default)]
    pub project_stage: ProjectStage,
    pub board: Board,
    pub participants: Vec<Participant>,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub sidechats: Vec<Sidechat>,
    pub timers: Vec<Timer>,
    #[serde(default)]
    pub claims: Vec<Claim>,
    #[serde(default)]
    pub requests: Vec<RoomRequest>,
    #[serde(default = "yes")]
    pub auto_continue: bool,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub runtime_error: Option<String>,
    #[serde(default)]
    pub origin: Option<RoomOrigin>,
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomOrigin {
    pub run_id: String,
    pub provider: String,
    pub session_id: String,
    pub title: String,
    pub message_count: usize,
    pub context: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateRoomInput {
    pub title: String,
    pub objective: String,
    pub repo_path: String,
    pub repository: String,
    pub create_project: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AddParticipantInput {
    pub name: String,
    pub provider: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub use_worktree: bool,
    pub max_turns: u32,
}

#[derive(Debug, Deserialize)]
pub struct SaveTimerInput {
    pub id: Option<String>,
    pub participant_id: String,
    pub message: String,
    pub interval_seconds: u64,
    pub idle_only: bool,
    pub enabled: bool,
    #[serde(default)]
    pub max_deliveries: Option<u32>,
    #[serde(default)]
    pub ends_at: Option<i64>,
}

impl Room {
    pub fn project_title(&self) -> String {
        format!("{} [room:{}]", self.title, self.id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomRequest {
    pub id: String,
    pub kind: String,
    pub requester_id: String,
    pub title: String,
    pub body: String,
    pub evidence: Option<String>,
    pub task_id: Option<String>,
    pub reviewer_id: Option<String>,
    pub proposal: Option<AddParticipantInput>,
    pub brief: Option<String>,
    pub options: Vec<String>,
    pub status: String,
    pub response: Option<String>,
    pub resolved_by: Option<String>,
    #[serde(default)]
    pub review_response: Option<String>,
    #[serde(default)]
    pub reviewed_by: Option<String>,
    #[serde(default)]
    pub reviewed_at: Option<String>,
    pub approved_participant_id: Option<String>,
    #[serde(default)]
    pub work_signature: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateRequestInput {
    pub kind: String,
    pub title: String,
    pub body: String,
    pub evidence: Option<String>,
    pub task_id: Option<String>,
    pub reviewer_id: Option<String>,
    pub proposal: Option<AddParticipantInput>,
    pub brief: Option<String>,
    #[serde(default)]
    pub options: Vec<String>,
}

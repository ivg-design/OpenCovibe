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
    pub message_cursor: usize,
    #[serde(default)]
    pub pending_delivery: Option<Delivery>,
    #[serde(default)]
    pub no_progress_turns: u32,
    #[serde(default)]
    pub work_signature: Option<String>,
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
pub struct Message {
    pub id: String,
    pub sender: String,
    pub body: String,
    pub created_at: String,
    #[serde(default)]
    pub participant_id: Option<String>,
    #[serde(default)]
    pub target_participant_id: Option<String>,
    #[serde(default)]
    pub source_event_id: Option<String>,
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
    pub max_deliveries: u32,
    pub delivered_count: u32,
    pub last_error: Option<String>,
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
    pub timers: Vec<Timer>,
    #[serde(default)]
    pub claims: Vec<Claim>,
    #[serde(default = "yes")]
    pub auto_continue: bool,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub runtime_error: Option<String>,
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: u32,
}

#[derive(Debug, Deserialize)]
pub struct CreateRoomInput {
    pub title: String,
    pub objective: String,
    pub repo_path: String,
    pub repository: String,
    pub create_project: bool,
}

#[derive(Debug, Deserialize)]
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
    pub max_deliveries: u32,
}

impl Room {
    pub fn project_title(&self) -> String {
        format!("{} [room:{}]", self.title, self.id)
    }
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardItem {
    pub id: String,
    pub title: String,
    pub url: Option<String>,
    pub status: String,
    pub priority: Option<String>,
    pub agent: Option<String>,
    pub kind: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Board {
    pub items: Vec<BoardItem>,
    pub synced_at: Option<String>,
    pub error: Option<String>,
    pub total_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Participant {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub run_id: String,
    pub paused: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub sender: String,
    pub body: String,
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
}

#[derive(Debug, Deserialize)]
pub struct CreateRoomInput {
    pub title: String,
    pub objective: String,
    pub repo_path: String,
    pub repository: String,
    pub create_project: bool,
}

impl Room {
    pub fn project_title(&self) -> String {
        format!("{} [room:{}]", self.title, self.id)
    }
}

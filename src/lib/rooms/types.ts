export interface RoomProject {
  id: string;
  number: number;
  title: string;
  url: string;
  owner: string;
  repository: string;
}

export interface RoomBoardItem {
  id: string;
  title: string;
  url: string | null;
  status: string;
  priority: string | null;
  agent: string | null;
  kind: "issue" | "pull_request" | "draft" | "redacted";
}

export interface RoomBoard {
  items: RoomBoardItem[];
  synced_at: string | null;
  error: string | null;
  total_count: number;
}

export interface RoomParticipant {
  id: string;
  name: string;
  provider: "codex" | "claude";
  run_id: string;
  paused: boolean;
  model: string | null;
  effort: string | null;
  worktree_path: string | null;
  branch: string | null;
  state: string;
  last_error: string | null;
  last_wake_at: number | null;
  wake_count: number;
  max_turns: number;
  event_cursor: number;
  message_cursor: number;
  pending_delivery: RoomDelivery | null;
}

export interface RoomDelivery {
  id: string;
  reason: string;
  text: string;
  created_at: number;
  state: string;
  task_id: string | null;
  timer_id: string | null;
}

export interface RoomClaim {
  task_id: string;
  participant_id: string;
  state: string;
  updated_at: string;
  summary: string | null;
  evidence: string | null;
}

export interface RoomMessage {
  id: string;
  sender: string;
  body: string;
  created_at: string;
  participant_id: string | null;
  target_participant_id: string | null;
  source_event_id: string | null;
}

export interface RoomTimer {
  id: string;
  participant_id: string;
  message: string;
  interval_seconds: number;
  idle_only: boolean;
  enabled: boolean;
  next_due_at: number;
  max_deliveries: number;
  delivered_count: number;
  last_error: string | null;
}

export interface Room {
  id: string;
  title: string;
  objective: string;
  repo_path: string;
  repository: string;
  created_at: string;
  updated_at: string;
  paused: boolean;
  project: RoomProject | null;
  board: RoomBoard;
  participants: RoomParticipant[];
  messages: RoomMessage[];
  timers: RoomTimer[];
  claims: RoomClaim[];
  auto_continue: boolean;
  archived: boolean;
  runtime_error: string | null;
}

export interface AddParticipantInput {
  name: string;
  provider: "codex" | "claude";
  model: string | null;
  effort: string | null;
  use_worktree: boolean;
  max_turns: number;
}

export interface SaveTimerInput {
  id: string | null;
  participant_id: string;
  message: string;
  interval_seconds: number;
  idle_only: boolean;
  enabled: boolean;
  max_deliveries: number;
}

export interface CreateRoomInput {
  title: string;
  objective: string;
  repo_path: string;
  repository: string;
  create_project: boolean;
}

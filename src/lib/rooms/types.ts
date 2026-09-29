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
}

export interface RoomMessage {
  id: string;
  sender: string;
  body: string;
  created_at: string;
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
}

export interface CreateRoomInput {
  title: string;
  objective: string;
  repo_path: string;
  repository: string;
  create_project: boolean;
}

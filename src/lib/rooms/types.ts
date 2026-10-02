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
  body?: string | null;
  number?: number | null;
  assignees?: string[];
  labels?: string[];
  linked_prs?: string[];
  updated_at?: string | null;
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
  no_progress_turns: number;
  work_signature: string | null;
  brief?: string | null;
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

export interface RoomAttachment {
  id: string;
  name: string;
  mime_type: string;
  size: number;
}

export interface RoomMessage {
  id: string;
  sender: string;
  body: string;
  created_at: string;
  participant_id: string | null;
  target_participant_id: string | null;
  target_participant_ids?: string[];
  source_event_id: string | null;
  sidechat_id?: string | null;
  attachments?: RoomAttachment[];
}

export interface RoomSidechat {
  id: string;
  title: string;
  source_message_id: string;
  participant_ids: string[];
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
  max_deliveries: number | null;
  ends_at?: number | null;
  delivered_count: number;
  queued_at: number | null;
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
  sidechats?: RoomSidechat[];
  timers: RoomTimer[];
  claims: RoomClaim[];
  requests: RoomRequest[];
  auto_continue: boolean;
  max_concurrent: number;
  archived: boolean;
  runtime_error: string | null;
  origin?: {
    run_id: string;
    provider: string;
    session_id: string;
    title: string;
    message_count: number;
    context: string;
  } | null;
}

export interface RoomSessionSeed {
  run_id: string;
  title: string;
  objective: string;
  repo_path: string;
  repository: string;
  provider: string;
  existing_room_id: string | null;
  projects: RoomProject[];
  project_error: string | null;
}

export interface GitHubRepository {
  remote: string;
  repository: string;
}

export interface RepositoryInspection {
  repo_path: string;
  repository: string;
  repositories: GitHubRepository[];
}

export interface ParticipantSettings {
  name: string;
  model: string | null;
  effort: string | null;
  /** Zero means no optional room turn limit. */
  max_turns: number;
}

export interface AddParticipantInput {
  name: string;
  provider: "codex" | "claude";
  model: string | null;
  effort: string | null;
  use_worktree: boolean;
  max_turns: number;
}

export type RoomRequestKind = "agent" | "decision" | "review" | "completion";
export type RoomRequestStatus =
  | "pending"
  | "creating"
  | "approved"
  | "rejected"
  | "changes_requested"
  | "verified"
  | "accepted";

export interface RoomRequest {
  id: string;
  kind: RoomRequestKind;
  requester_id: string;
  title: string;
  body: string;
  evidence: string | null;
  task_id: string | null;
  reviewer_id: string | null;
  proposal: AddParticipantInput | null;
  brief: string | null;
  options: string[];
  status: RoomRequestStatus;
  response: string | null;
  resolved_by: string | null;
  review_response?: string | null;
  reviewed_by?: string | null;
  reviewed_at?: string | null;
  approved_participant_id: string | null;
  work_signature: string | null;
  created_at: string;
  updated_at: string;
}

export interface SaveTimerInput {
  id: string | null;
  participant_id: string;
  message: string;
  interval_seconds: number;
  idle_only: boolean;
  enabled: boolean;
  max_deliveries: number | null;
  ends_at?: number | null;
}

export interface CreateRoomInput {
  title: string;
  objective: string;
  repo_path: string;
  repository: string;
  create_project: boolean;
}

export interface RoomSidebarEntry {
  id: string;
  title: string;
  repo_path: string;
  updated_at: string;
  needs_answer: number;
  participants: {
    run_id: string;
    participant_id: string;
    name: string;
    provider: string;
    state: string;
    color_index: number;
  }[];
}

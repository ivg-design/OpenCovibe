import { getTransport } from "$lib/transport";
import type {
  AddParticipantInput,
  ParticipantSettings,
  CreateRoomInput,
  Room,
  RoomSessionSeed,
  RepositoryInspection,
  SaveTimerInput,
} from "./types";

export function listRooms(): Promise<Room[]> {
  return getTransport().invoke("list_rooms");
}

export function getRoom(id: string): Promise<Room> {
  return getTransport().invoke("get_room", { id });
}

export function createRoom(input: CreateRoomInput): Promise<Room> {
  return getTransport().invoke("create_room", { input });
}

export function getRoomSessionSeed(runId: string): Promise<RoomSessionSeed> {
  return getTransport().invoke("get_room_session_seed", { runId });
}

export function inspectRoomRepository(path: string): Promise<RepositoryInspection> {
  return getTransport().invoke("inspect_room_repository", { path });
}

export function createRoomFromSession(
  runId: string,
  input: CreateRoomInput,
  projectId: string | null = null,
): Promise<Room> {
  return getTransport().invoke("create_room_from_session", { runId, input, projectId });
}

export function refreshRoomBoard(id: string): Promise<Room> {
  return getTransport().invoke("refresh_room_board", { id });
}

export function ensureRoomProject(id: string): Promise<Room> {
  return getTransport().invoke("ensure_room_project", { id });
}

export function setRoomPaused(id: string, paused: boolean): Promise<Room> {
  return getTransport().invoke("set_room_paused", { id, paused });
}

export function postRoomMessage(
  id: string,
  body: string,
  targetParticipantId: string | null = null,
  sidechatId: string | null = null,
): Promise<Room> {
  return getTransport().invoke("post_room_message", { id, body, targetParticipantId, sidechatId });
}

export function createRoomSidechat(
  id: string,
  sourceMessageId: string,
  title: string,
  participantIds: string[],
): Promise<Room> {
  return getTransport().invoke("create_room_sidechat", {
    id,
    sourceMessageId,
    title,
    participantIds,
  });
}

export function addRoomParticipant(id: string, input: AddParticipantInput): Promise<Room> {
  return getTransport().invoke("add_room_participant", { id, input });
}
export function setRoomParticipantPaused(
  id: string,
  participantId: string,
  paused: boolean,
): Promise<Room> {
  return getTransport().invoke("set_room_participant_paused", { id, participantId, paused });
}
export function wakeRoomParticipant(
  id: string,
  participantId: string,
  message: string,
): Promise<Room> {
  return getTransport().invoke("wake_room_participant", { id, participantId, message });
}
export function removeRoomParticipant(id: string, participantId: string): Promise<Room> {
  return getTransport().invoke("remove_room_participant", { id, participantId });
}
export function saveRoomTimer(id: string, input: SaveTimerInput): Promise<Room> {
  return getTransport().invoke("save_room_timer", { id, input });
}
export function removeRoomTimer(id: string, timerId: string): Promise<Room> {
  return getTransport().invoke("remove_room_timer", { id, timerId });
}
export function setRoomAutoContinue(id: string, enabled: boolean): Promise<Room> {
  return getTransport().invoke("set_room_auto_continue", { id, enabled });
}
export function attachRoomProject(id: string, number: number): Promise<Room> {
  return getTransport().invoke("attach_room_project", { id, number });
}
export function archiveRoom(id: string): Promise<Room> {
  return getTransport().invoke("archive_room", { id });
}
export function releaseRoomClaim(id: string, taskId: string): Promise<Room> {
  return getTransport().invoke("release_room_claim", { id, taskId });
}
export function mergeRoomWorktree(id: string, participantId: string): Promise<Room> {
  return getTransport().invoke("merge_room_worktree", { id, participantId });
}

export function readRoomTask(
  id: string,
  taskId: string,
): Promise<{ id: string; title: string; body: string; url: string | null; kind: string }> {
  return getTransport().invoke("read_room_task", { id, taskId });
}

export function setRoomConcurrency(id: string, limit: number): Promise<Room> {
  return getTransport().invoke("set_room_concurrency", { id, limit });
}

export function resolveRoomRequest(
  id: string,
  requestId: string,
  approve: boolean,
  response: string,
): Promise<Room> {
  return getTransport().invoke("resolve_room_request", { id, requestId, approve, response });
}

export function approveRoomAgent(id: string, requestId: string): Promise<Room> {
  return getTransport().invoke("approve_room_agent", { id, requestId });
}

export function saveRoomInstructions(
  id: string,
  instructions: string,
  expected: string,
): Promise<Room> {
  return getTransport().invoke("save_room_instructions", { id, instructions, expected });
}

export function updateRoomParticipantSettings(
  id: string,
  participantId: string,
  input: ParticipantSettings,
  expected: ParticipantSettings,
): Promise<Room> {
  return getTransport().invoke("update_room_participant_settings", {
    id,
    participantId,
    input,
    expected,
  });
}

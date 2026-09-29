import { getTransport } from "$lib/transport";
import type { AddParticipantInput, CreateRoomInput, Room, SaveTimerInput } from "./types";

export function listRooms(): Promise<Room[]> {
  return getTransport().invoke("list_rooms");
}

export function getRoom(id: string): Promise<Room> {
  return getTransport().invoke("get_room", { id });
}

export function createRoom(input: CreateRoomInput): Promise<Room> {
  return getTransport().invoke("create_room", { input });
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
): Promise<Room> {
  return getTransport().invoke("post_room_message", { id, body, targetParticipantId });
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

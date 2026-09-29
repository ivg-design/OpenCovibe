import { getTransport } from "$lib/transport";
import type { CreateRoomInput, Room } from "./types";

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

export function postRoomMessage(id: string, body: string): Promise<Room> {
  return getTransport().invoke("post_room_message", { id, body });
}

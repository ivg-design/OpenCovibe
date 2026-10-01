import type { RoomParticipant } from "$lib/rooms/types";

/** Shared participant colors for the room transcript and its linked sidebar sessions. */
export const ROOM_PARTICIPANT_COLORS = [
  "#5677c8",
  "#b36a9a",
  "#538b70",
  "#bd8250",
  "#7481aa",
  "#ad665d",
] as const;

export function roomParticipantColorAt(index: number, participantId = ""): string {
  if (index >= 0) return ROOM_PARTICIPANT_COLORS[index % ROOM_PARTICIPANT_COLORS.length];
  let hash = 0;
  for (const char of participantId) hash = (hash * 31 + char.charCodeAt(0)) | 0;
  return ROOM_PARTICIPANT_COLORS[Math.abs(hash) % ROOM_PARTICIPANT_COLORS.length];
}

export function roomParticipantColor(
  participants: readonly Pick<RoomParticipant, "id">[],
  participantId: string,
): string {
  return roomParticipantColorAt(
    participants.findIndex((participant) => participant.id === participantId),
    participantId,
  );
}

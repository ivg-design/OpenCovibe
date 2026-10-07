import type { RoomMessage } from "$lib/rooms/types";

let displayName = $state("Human");

export function setIdentityName(name?: string | null): void {
  displayName = name?.trim() || "Human";
}

export function identityName(): string {
  return displayName;
}

/** Keep the stable Human role and participant identities separate from display labels. */
export function roomSenderName(message: Pick<RoomMessage, "sender" | "participant_id">): string {
  return message.sender === "Human" && !message.participant_id ? displayName : message.sender;
}

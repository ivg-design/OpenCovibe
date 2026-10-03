import type { Room, RoomRequest } from "./types";

/** Match the response field's action without bypassing paused-room or review gates. */
export function requestReplyApproval(
  room: Pick<Room, "archived" | "paused" | "participants">,
  request: RoomRequest,
  response: string,
  disabled: boolean,
): boolean | null {
  if (disabled || room.archived || request.archived || !response.trim()) return null;
  if (request.kind === "decision" && request.status === "pending") return true;
  if (request.kind === "completion" && request.status === "verified")
    return room.paused ? true : null;
  if (
    ["review", "completion"].includes(request.kind) &&
    ["pending", "changes_requested"].includes(request.status)
  )
    return false;
  if (
    request.kind === "agent" &&
    request.status === "pending" &&
    !request.approved_participant_id &&
    !room.participants.some((peer) => peer.id === request.id)
  )
    return false;
  return null;
}

export function isRequestReplyShortcut(
  event: Pick<
    KeyboardEvent,
    "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey" | "isComposing" | "repeat"
  >,
): boolean {
  return (
    event.key === "Enter" &&
    (event.metaKey || event.ctrlKey) &&
    !event.altKey &&
    !event.shiftKey &&
    !event.isComposing &&
    !event.repeat
  );
}

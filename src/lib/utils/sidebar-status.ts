import type { MessageKey } from "$lib/i18n/types";
import type { RoomSidebarEntry } from "$lib/rooms/types";
import type { RunStatus } from "$lib/types";

export function sidebarAgentStatus(
  state: string,
  attention = false,
): { status: RunStatus; label: MessageKey } {
  if (attention && ["running", "busy", "idle", "completed"].includes(state))
    return { status: "pending", label: "room_stateWaiting" };
  if (["busy", "running"].includes(state)) return { status: "running", label: "room_stateBusy" };
  if (["idle", "completed"].includes(state)) return { status: "idle", label: "room_stateIdle" };
  if (["paused", "stopped", "no_progress", "budget_exhausted"].includes(state))
    return { status: "stopped", label: "room_statePaused" };
  if (state === "failed") return { status: "failed", label: "room_stateFailed" };
  if (state === "blocked") return { status: "pending", label: "room_stateBlocked" };
  if (state === "waiting") return { status: "pending", label: "room_stateWaiting" };
  if (["spawning", "starting", "pending"].includes(state))
    return { status: "pending", label: "room_stateStarting" };
  return { status: "pending", label: "room_stateUnknown" };
}

export function sidebarRoomState(room: RoomSidebarEntry): string {
  if (room.paused) return "paused";
  const states = room.participants.map((peer) => peer.state);
  if (states.some((state) => ["busy", "running"].includes(state))) return "busy";
  if (states.some((state) => ["spawning", "starting", "pending"].includes(state)))
    return "starting";
  if (room.needs_answer || states.includes("waiting")) return "waiting";
  if (states.includes("failed")) return "failed";
  if (states.includes("blocked")) return "blocked";
  if (
    states.length &&
    states.every((state) =>
      ["paused", "stopped", "no_progress", "budget_exhausted"].includes(state),
    )
  )
    return "paused";
  return "idle";
}

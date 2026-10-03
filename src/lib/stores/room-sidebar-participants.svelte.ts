import { getTransport } from "$lib/transport";
import type { RoomSidebarEntry } from "$lib/rooms/types";
import { visibleRooms, visibleRoomRuns, roomVisibilityEvent } from "$lib/rooms/visibility";
import type { TaskRun } from "$lib/types";

export interface RoomSidebarParticipant {
  room_id: string;
  run_id: string;
  participant_id: string;
  name: string;
  color_index: number;
}

let entries = $state<RoomSidebarEntry[]>([]);
let visibilityRevision = $state(0);
export function roomSidebarEntries(): RoomSidebarEntry[] {
  void visibilityRevision;
  return visibleRooms(entries);
}
export function roomSidebarRuns(runs: TaskRun[]): TaskRun[] {
  return visibleRoomRuns(runs, entries, roomSidebarEntries());
}
function visibilityChanged() {
  visibilityRevision++;
}

let byRunId = $state<Record<string, RoomSidebarParticipant>>({});
let references = 0;
let timer: ReturnType<typeof setInterval> | undefined;
let refreshRequest = 0;

async function refreshRoomParticipants() {
  const request = ++refreshRequest;
  try {
    const rooms = await getTransport().invoke<RoomSidebarEntry[]>("list_room_sidebar_entries");
    if (request !== refreshRequest) return;
    const next: Record<string, RoomSidebarParticipant> = {};
    for (const room of rooms)
      for (const peer of room.participants) next[peer.run_id] = { ...peer, room_id: room.id };
    entries = rooms;
    byRunId = next;
  } catch {
    // Keep the last successful mapping; temporary room-service failures should not hide labels.
  }
}

/** Share one room status poll across all sidebar conversation rows. */
export function retainRoomSidebarParticipants(): () => void {
  references++;
  if (references === 1) {
    void refreshRoomParticipants();
    timer = setInterval(() => void refreshRoomParticipants(), 5_000);
    window.addEventListener("ocv:room-changed", refreshRoomParticipants);
    window.addEventListener(roomVisibilityEvent, visibilityChanged);
  }
  let released = false;
  return () => {
    if (released) return;
    released = true;
    references = Math.max(0, references - 1);
    if (references === 0) {
      if (timer) clearInterval(timer);
      timer = undefined;
      window.removeEventListener("ocv:room-changed", refreshRoomParticipants);
      window.removeEventListener(roomVisibilityEvent, visibilityChanged);
      refreshRequest++;
    }
  };
}

export function roomSidebarParticipant(runId: string): RoomSidebarParticipant | undefined {
  return byRunId[runId];
}

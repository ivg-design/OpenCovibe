import { listRoomAgentIdentities } from "$lib/rooms/api";

export interface RoomSidebarParticipant {
  room_id: string;
  run_id: string;
  participant_id: string;
  name: string;
  color_index: number;
}

let byRunId = $state<Record<string, RoomSidebarParticipant>>({});
let references = 0;
let timer: ReturnType<typeof setInterval> | undefined;
let refreshRequest = 0;

async function refreshRoomParticipants() {
  const request = ++refreshRequest;
  try {
    const identities = await listRoomAgentIdentities();
    if (request !== refreshRequest) return;
    const next: Record<string, RoomSidebarParticipant> = {};
    for (const identity of identities) next[identity.run_id] = identity;
    byRunId = next;
  } catch {
    // Keep the last successful mapping; temporary room-service failures should not hide labels.
  }
}

/** Share one low-frequency room poll across all sidebar conversation rows. */
export function retainRoomSidebarParticipants(): () => void {
  references++;
  if (references === 1) {
    void refreshRoomParticipants();
    timer = setInterval(() => void refreshRoomParticipants(), 15_000);
    window.addEventListener("ocv:room-changed", refreshRoomParticipants);
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
      refreshRequest++;
    }
  };
}

export function roomSidebarParticipant(runId: string): RoomSidebarParticipant | undefined {
  return byRunId[runId];
}

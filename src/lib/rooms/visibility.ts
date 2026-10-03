import { loadRemovedCwds } from "$lib/utils/removed-cwds";
import { normalizeCwd } from "$lib/utils/sidebar-groups";

type RoomLocation = { id: string; repo_path: string; archived?: boolean };
const hiddenKey = "ocv:hidden-room-ids";
export const roomVisibilityEvent = "ocv:room-visibility-changed";

export function hiddenRoomIds(): string[] {
  try {
    const value = JSON.parse(localStorage.getItem(hiddenKey) ?? "[]");
    return Array.isArray(value) ? value.filter((id): id is string => typeof id === "string") : [];
  } catch {
    return [];
  }
}

export function visibleRooms<T extends RoomLocation>(
  rooms: T[],
  removedFolders = loadRemovedCwds(),
  hiddenIds = hiddenRoomIds(),
): T[] {
  const removed = new Set(removedFolders.map(normalizeCwd));
  const hidden = new Set(hiddenIds);
  return rooms.filter(
    (room) => !room.archived && !hidden.has(room.id) && !removed.has(normalizeCwd(room.repo_path)),
  );
}

export function visibleRoomRuns<T extends { id: string; session_id?: string | null }>(
  runs: T[],
  rooms: (RoomLocation & { participants: { run_id: string }[] })[],
  available: RoomLocation[],
): T[] {
  const visible = new Set(available.map((room) => room.id));
  const hiddenRuns = new Set(
    rooms
      .filter((room) => !visible.has(room.id))
      .flatMap((room) => room.participants.map((peer) => peer.run_id)),
  );
  const hiddenSessions = new Set(
    runs.filter((run) => hiddenRuns.has(run.id) && run.session_id).map((run) => run.session_id),
  );
  return runs.filter(
    (run) => !hiddenRuns.has(run.id) && !(run.session_id && hiddenSessions.has(run.session_id)),
  );
}

/** Reveal one room without bringing back older rooms from a removed project. */
export function revealRoom(room: RoomLocation, allRooms: RoomLocation[]) {
  const removed = loadRemovedCwds();
  const hidden = new Set(hiddenRoomIds());
  const cwd = normalizeCwd(room.repo_path);
  if (removed.includes(cwd)) {
    for (const other of allRooms)
      if (normalizeCwd(other.repo_path) === cwd && other.id !== room.id) hidden.add(other.id);
  }
  hidden.delete(room.id);
  localStorage.setItem(hiddenKey, JSON.stringify([...hidden]));
  localStorage.setItem("ocv:removed-cwds", JSON.stringify(removed.filter((path) => path !== cwd)));
  window.dispatchEvent(new Event(roomVisibilityEvent));
}

/**
 * Sidebar grouping utilities — pure functions for building the project folder tree.
 *
 * Transforms a flat list of TaskRun into ProjectFolder[] where each folder
 * contains ConversationGroup[] (runs grouped by session_id).
 */

import type { RoomSidebarEntry } from "$lib/rooms/types";
import type { TaskRun } from "$lib/types";
import { cwdDisplayLabel } from "$lib/utils/format";
import { roomChatTitle } from "$lib/utils/room-presentation";

// ── Public types ──

export interface ConversationGroup {
  groupKey: string; // "s:<session_id>" or "r:<run.id>"
  runs: TaskRun[]; // sorted by started_at desc
  title: string;
  latestRun: TaskRun;
  isFavorite: boolean;
  totalMessages: number;
}

export interface ProjectFolder {
  cwd: string; // "" = uncategorized
  folderKey: string; // "uncategorized" or "cwd:<path>"
  isUncategorized: boolean;
  conversations: ConversationGroup[];
  conversationCount: number;
  rooms?: RoomSidebarEntry[];
  latestActivityAt: string; // last_activity_at ?? started_at (safe)
}

/** Rooms and direct conversations share one alphabetical sibling list. */
export function projectSidebarEntries(folder: ProjectFolder) {
  return [
    ...(folder.rooms ?? []).map((room) => ({
      kind: "room" as const,
      key: `room:${room.id}`,
      title: room.title,
      room,
    })),
    ...folder.conversations.map((conversation) => ({
      kind: "conversation" as const,
      key: conversation.groupKey,
      title: conversation.title,
      conversation,
    })),
  ].sort((a, b) => compareSidebarLabels(a.title, b.title) || compareSidebarLabels(a.key, b.key));
}

// ── normalizeCwd ──

/** Normalize cwd: unify separators + strip trailing + uppercase drive; empty/"/"/"\" → "" */
export function normalizeCwd(cwd: string | undefined): string {
  let s = (cwd ?? "").trim();
  if (!s || s === "/" || s === "\\") return "";
  // Windows: backslash → forward slash
  s = s.replace(/\\/g, "/");
  // Windows: drive letter uppercase (c:/Repo → C:/Repo)
  s = s.replace(/^([a-z]):/, (_, d: string) => d.toUpperCase() + ":");
  // Bare drive letter "C:" → "C:/"
  if (/^[A-Z]:$/.test(s)) return s + "/";
  // Preserve drive root "C:/"
  if (/^[A-Z]:\/$/.test(s)) return s;
  // Preserve UNC root "//server" (strip trailing slash if "//server/")
  if (/^\/\/[^/]+\/?$/.test(s)) return s.replace(/\/$/, "");
  // Strip trailing slashes
  return s.replace(/\/+$/, "");
}

// Compare visible labels without activity timestamps; numeric names sort naturally.
const sidebarCollator = new Intl.Collator(undefined, { sensitivity: "base", numeric: true });
export function compareSidebarLabels(a: string, b: string): number {
  return sidebarCollator.compare(a, b) || (a < b ? -1 : a > b ? 1 : 0);
}

// ── Activity metadata helper ──

function sortKey(run: TaskRun): string {
  return run.last_activity_at ?? run.started_at;
}

// ── Main grouping function ──

export function buildProjectFolders(
  runs: TaskRun[],
  favoriteRunIds: Set<string>,
  pinnedCwds: string[],
  removedCwds: string[] = [],
  rooms: RoomSidebarEntry[] = [],
): ProjectFolder[] {
  // 1. Build removed set (empty string excluded — Uncategorized never removed)
  const removedSet = new Set(removedCwds.map(normalizeCwd));
  removedSet.delete("");

  // 2. Clean pinnedCwds — normalize + filter empty + filter removed
  const cleanPinned = pinnedCwds.map(normalizeCwd).filter((c) => c !== "" && !removedSet.has(c));

  // 3. Bucket runs by normalized cwd
  const cwdBuckets = new Map<string, TaskRun[]>();
  const roomForRun = new Map(
    rooms.flatMap((room) => room.participants.map((peer) => [peer.run_id, room] as const)),
  );
  const roomForSession = new Map(
    runs
      .filter((run) => run.session_id && roomForRun.has(run.id))
      .map((run) => [run.session_id!, roomForRun.get(run.id)!] as const),
  );
  for (const run of runs) {
    const room =
      roomForRun.get(run.id) ?? (run.session_id ? roomForSession.get(run.session_id) : undefined);
    const cwd = normalizeCwd(room?.repo_path ?? run.cwd);
    let bucket = cwdBuckets.get(cwd);
    if (!bucket) {
      bucket = [];
      cwdBuckets.set(cwd, bucket);
    }
    bucket.push(run);
  }

  for (const room of rooms) {
    const cwd = normalizeCwd(room.repo_path);
    if (!cwdBuckets.has(cwd)) cwdBuckets.set(cwd, []);
  }

  // 4. Remove buckets in removedSet
  for (const cwd of removedSet) {
    cwdBuckets.delete(cwd);
  }

  // 5. Ensure pinned cwds have entries (even if empty)
  for (const cwd of cleanPinned) {
    if (!cwdBuckets.has(cwd)) {
      cwdBuckets.set(cwd, []);
    }
  }

  // 6. Build folders
  const folders: ProjectFolder[] = [];

  for (const [cwd, bucketRuns] of cwdBuckets) {
    const isUncategorized = cwd === "";
    const folderKey = isUncategorized ? "uncategorized" : `cwd:${cwd}`;

    // Group runs by session_id within this cwd
    const sessionMap = new Map<string, TaskRun[]>();
    const standalone: TaskRun[] = [];

    for (const run of bucketRuns) {
      if (run.session_id) {
        let group = sessionMap.get(run.session_id);
        if (!group) {
          group = [];
          sessionMap.set(run.session_id, group);
        }
        group.push(run);
      } else {
        standalone.push(run);
      }
    }

    // Build conversation groups
    const conversations: ConversationGroup[] = [];

    // Session-based groups
    for (const [sessionId, sessionRuns] of sessionMap) {
      // Sort runs by started_at desc
      sessionRuns.sort((a, b) => b.started_at.localeCompare(a.started_at));
      const latestRun = sessionRuns[0];
      const earliestRun = sessionRuns[sessionRuns.length - 1];
      const title = roomChatTitle(
        latestRun.name?.trim() || earliestRun.prompt?.trim() || "Untitled",
        earliestRun.prompt,
      );
      const isFavorite = sessionRuns.some((r) => favoriteRunIds.has(r.id));
      const totalMessages = sessionRuns.reduce((sum, r) => sum + (r.message_count ?? 0), 0);

      conversations.push({
        groupKey: `s:${sessionId}`,
        runs: sessionRuns,
        title,
        latestRun,
        isFavorite,
        totalMessages,
      });
    }

    // Standalone runs (no session_id)
    for (const run of standalone) {
      const title = roomChatTitle(run.name?.trim() || run.prompt?.trim() || "Untitled", run.prompt);
      conversations.push({
        groupKey: `r:${run.id}`,
        runs: [run],
        title,
        latestRun: run,
        isFavorite: favoriteRunIds.has(run.id),
        totalMessages: run.message_count ?? 0,
      });
    }

    // Stable alphabetical positions even while messages and statuses refresh.
    conversations.sort(
      (a, b) =>
        compareSidebarLabels(a.title, b.title) || compareSidebarLabels(a.groupKey, b.groupKey),
    );

    const folderRooms = rooms
      .filter((room) => normalizeCwd(room.repo_path) === cwd)
      .map((room) => ({
        ...room,
        participants: [...room.participants].sort(
          (a, b) =>
            compareSidebarLabels(a.name, b.name) ||
            compareSidebarLabels(a.participant_id, b.participant_id),
        ),
      }))
      .sort((a, b) => compareSidebarLabels(a.title, b.title) || compareSidebarLabels(a.id, b.id));
    const standaloneConversations = conversations.filter(
      (conv) => !conv.runs.some((run) => roomForRun.has(run.id)),
    );
    const latestActivityAt =
      [
        ...conversations.map((conversation) => sortKey(conversation.latestRun)),
        ...folderRooms.map((room) => room.updated_at),
      ]
        .sort()
        .at(-1) ?? "";

    folders.push({
      cwd,
      folderKey,
      isUncategorized,
      conversations: standaloneConversations,
      rooms: folderRooms,
      conversationCount: standaloneConversations.length + folderRooms.length,
      latestActivityAt,
    });
  }

  // 7. Sort by the displayed project name, with paths breaking duplicate-name ties.
  folders.sort((a, b) => {
    if (a.isUncategorized && !b.isUncategorized) return 1;
    if (!a.isUncategorized && b.isUncategorized) return -1;
    return (
      compareSidebarLabels(cwdDisplayLabel(a.cwd), cwdDisplayLabel(b.cwd)) ||
      compareSidebarLabels(a.folderKey, b.folderKey)
    );
  });

  return folders;
}

// ── Expand helpers ──

/** Auto-expand the folder containing selectedRunId. Returns new Set or null (no change). */
export function autoExpandForRun(
  selectedRunId: string | undefined,
  projectFolders: ProjectFolder[],
  expandedProjects: Set<string>,
): Set<string> | null {
  if (!selectedRunId) return null;

  for (const folder of projectFolders) {
    const found = folder.conversations.some((conv) =>
      conv.runs.some((r) => r.id === selectedRunId),
    );
    if (
      found ||
      folder.rooms?.some((room) => room.participants.some((peer) => peer.run_id === selectedRunId))
    ) {
      if (expandedProjects.has(folder.folderKey)) return null; // already expanded
      const next = new Set(expandedProjects);
      next.add(folder.folderKey);
      return next;
    }
  }

  return null;
}

/** Expand a folder by its folderKey. Returns new Set or null (skip). */
export function expandForProjectChange(
  folderKey: string,
  expandedProjects: Set<string>,
): Set<string> | null {
  if (!folderKey) return null; // empty = "All Projects" → skip
  if (expandedProjects.has(folderKey)) return null; // already expanded
  const next = new Set(expandedProjects);
  next.add(folderKey);
  return next;
}

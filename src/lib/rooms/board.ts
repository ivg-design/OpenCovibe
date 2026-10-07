import type { RoomBoardItem, RoomClaim, RoomParticipant } from "./types";

export interface BoardFilters {
  title: string;
  priority: string;
  agent: string;
  status?: string;
}

export interface BoardGroup {
  status: string;
  items: RoomBoardItem[];
}

/** Redacted rows carry no displayable metadata, even if the server included it. */
export function visibleBoardItems(items: RoomBoardItem[], filters: BoardFilters): RoomBoardItem[] {
  const query = (value: string) => value.trim().toLocaleLowerCase();
  const title = query(filters.title);
  const priority = query(filters.priority);
  const agent = query(filters.agent);

  return items.filter((item) => {
    if (item.kind === "redacted") return true;
    return (
      (!title ||
        [item.title, item.body, item.number, ...(item.labels ?? []), ...(item.linked_prs ?? [])]
          .join(" ")
          .toLocaleLowerCase()
          .includes(title)) &&
      (!priority || (item.priority ?? "").toLocaleLowerCase().includes(priority)) &&
      (!agent ||
        [item.agent, ...(item.assignees ?? [])].join(" ").toLocaleLowerCase().includes(agent)) &&
      (!filters.status || item.status === filters.status)
    );
  });
}

export function groupBoardItems(items: RoomBoardItem[]): BoardGroup[] {
  const groups = new Map<string, RoomBoardItem[]>();
  for (const item of items) {
    const status = item.kind === "redacted" ? "__hidden__" : item.status || "Unknown";
    const group = groups.get(status) ?? [];
    group.push(item);
    groups.set(status, group);
  }
  return [...groups.entries()]
    .sort(([a], [b]) => statusRank(a) - statusRank(b) || a.localeCompare(b))
    .map(([status, groupItems]) => ({ status, items: groupItems }));
}

export function statusRank(status: string): number {
  const s = status.toLowerCase();
  if (s.includes("block") || s.includes("attention")) return 0;
  if (s.includes("progress") || s === "active") return 1;
  if (s.includes("review") || s.includes("completing")) return 2;
  if (s === "ready" || s === "todo" || s === "reserved") return 3;
  if (s.includes("backlog") || s === "unknown") return 4;
  if (s === "done" || s === "closed" || s === "completed") return 6;
  if (s === "__hidden__") return 7;
  return 5;
}

export interface BoardTask extends RoomBoardItem {
  claim?: RoomClaim;
  ownerId?: string;
}

/** Live room ownership and blockers complement the last GitHub snapshot. */
export function boardTasks(
  items: RoomBoardItem[],
  claims: RoomClaim[],
  peers: RoomParticipant[],
): BoardTask[] {
  return items
    .map((item) => {
      if (item.kind === "redacted")
        return {
          ...item,
          title: "",
          body: null,
          status: "__hidden__",
          agent: null,
          priority: null,
          url: null,
          labels: [],
          assignees: [],
          linked_prs: [],
          number: null,
          updated_at: null,
        };
      const claim = [...claims]
        .reverse()
        .find((c) => c.task_id === item.id && c.state !== "released");
      const peer = peers.find((p) => p.id === claim?.participant_id);
      const status =
        claim?.state === "blocked"
          ? "Blocked"
          : claim?.state === "uncertain"
            ? "Needs attention"
            : item.status || "Unknown";
      return { ...item, status, agent: peer?.name ?? item.agent, ownerId: peer?.id, claim };
    })
    .sort(
      (a, b) =>
        statusRank(a.status) - statusRank(b.status) ||
        (a.priority ?? "zz").localeCompare(b.priority ?? "zz") ||
        a.title.localeCompare(b.title),
    );
}

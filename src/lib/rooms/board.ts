import type { RoomBoardItem } from "./types";

export interface BoardFilters {
  title: string;
  priority: string;
  agent: string;
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
      (!title || item.title.toLocaleLowerCase().includes(title)) &&
      (!priority || (item.priority ?? "").toLocaleLowerCase().includes(priority)) &&
      (!agent || (item.agent ?? "").toLocaleLowerCase().includes(agent))
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
    .sort(([a], [b]) => (a === "__hidden__" ? 1 : b === "__hidden__" ? -1 : a.localeCompare(b)))
    .map(([status, groupItems]) => ({ status, items: groupItems }));
}

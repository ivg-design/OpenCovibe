import { describe, expect, it } from "vitest";
import { groupBoardItems, visibleBoardItems } from "./board";
import type { RoomBoardItem } from "./types";

const rows: RoomBoardItem[] = [
  {
    id: "1",
    title: "Fix upload",
    url: "https://example.test/1",
    status: "Todo",
    priority: "high",
    agent: "Codex",
    kind: "issue",
  },
  {
    id: "2",
    title: "Private title must not appear",
    url: "https://example.test/private",
    status: "Secret status",
    priority: "urgent",
    agent: "Secret agent",
    kind: "redacted",
  },
  {
    id: "3",
    title: "Ship fix",
    url: null,
    status: "Done",
    priority: null,
    agent: null,
    kind: "draft",
  },
];

describe("room board presentation", () => {
  it("filters visible task fields while preserving generic redacted placeholders", () => {
    const filtered = visibleBoardItems(rows, { title: "upload", priority: "high", agent: "codex" });
    expect(filtered.map(({ id }) => id)).toEqual(["1", "2"]);
    expect(filtered[1].kind).toBe("redacted");
  });

  it("never groups redacted rows by their supplied status", () => {
    const groups = groupBoardItems(rows);
    expect(groups.map(({ status }) => status)).toEqual(["Done", "Todo", "__hidden__"]);
    expect(groups.at(-1)?.items).toEqual([rows[1]]);
  });
});

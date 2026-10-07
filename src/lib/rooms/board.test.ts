import { describe, expect, it } from "vitest";
import { boardTasks, groupBoardItems, visibleBoardItems } from "./board";
import type { RoomBoardItem, RoomClaim, RoomParticipant } from "./types";

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
    expect(groups.map(({ status }) => status)).toEqual(["Todo", "Done", "__hidden__"]);
    expect(groups.at(-1)?.items).toEqual([rows[1]]);
  });

  it("uses live blockers and renamed room owners without changing the GitHub snapshot", () => {
    const claim: RoomClaim = {
      task_id: "1",
      participant_id: "lead",
      state: "blocked",
      updated_at: "now",
      summary: "Waiting for export proof",
      evidence: "Commit abc123",
    };
    const peers = [{ id: "lead", name: "Lead Codex" }] as RoomParticipant[];
    const tasks = boardTasks(rows, [claim], peers);
    expect(tasks[0]).toMatchObject({
      id: "1",
      status: "Blocked",
      agent: "Lead Codex",
      ownerId: "lead",
      claim,
    });
    expect(rows[0].status).toBe("Todo");
    expect(tasks.at(-1)).toMatchObject({ kind: "redacted", title: "", agent: null, url: null });
  });

  it("finds task references and GitHub assignees in large projects", () => {
    const items = Array.from({ length: 480 }, (_, index) => ({
      ...rows[0],
      id: String(index),
      title: `Task ${index}`,
      body: index === 479 ? "Resolved in commit abc123" : "",
      number: index + 1,
      assignees: [index === 479 ? "reviewer" : "builder"],
      labels: ["runtime"],
      linked_prs: ["https://github.com/o/r/pull/27"],
    }));
    expect(
      visibleBoardItems(items, { title: "abc123", priority: "", agent: "reviewer" }).map(
        (i) => i.id,
      ),
    ).toEqual(["479"]);
    expect(visibleBoardItems(items, { title: "/pull/27", priority: "", agent: "" })).toHaveLength(
      480,
    );
    expect(
      visibleBoardItems(items, { title: "", priority: "", agent: "", status: "Done" }),
    ).toHaveLength(0);
  });
});

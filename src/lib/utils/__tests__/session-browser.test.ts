import { describe, it, expect } from "vitest";
import type { CliSessionSummary } from "$lib/types";
import {
  filterSessions,
  isBackgroundSession,
  sessionTitle,
  sessionProject,
  sessionPreview,
} from "../session-browser";

const main: CliSessionSummary = {
  agent: "codex",
  sessionId: "main",
  cwd: "/repo/worktree",
  projectPath: "/repo",
  title: "RAV 2.6.0 work",
  firstPrompt: "Original question about file size",
  startedAt: "",
  lastActivityAt: "",
  messageCount: 0,
  fileSize: 1,
  filePath: "",
  hasSubagents: true,
  alreadyImported: false,
};
describe("session discovery presentation", () => {
  it("finds a named main chat despite opening text and worktree location", () => {
    expect(sessionTitle(main)).toBe("RAV 2.6.0 work");
    expect(sessionProject(main)).toBe("/repo");
    expect(filterSessions([main], "RAV 2.6", false, false)).toEqual([main]);
  });
  it("hides children and archived chats by default without hiding parents that use agents", () => {
    const child = { ...main, sessionId: "child", isSubagent: true };
    const archived = { ...main, sessionId: "old", archived: true };
    expect(filterSessions([main, child, archived], "", false, false)).toEqual([main]);
    expect(filterSessions([main, child, archived], "", true, true)).toHaveLength(3);
    expect(filterSessions([{ ...main, isAutomated: true }], "", false, false)).toEqual([]);
  });
  it("renders encoded space and quotes as plain prose", () => {
    expect(sessionPreview({ ...main, firstPrompt: "A&#x20;note &quot;here&quot;" })).toBe(
      'A note "here"',
    );
  });
  it("never displays schema-only opening text as a chat title", () => {
    expect(sessionTitle({ ...main, title: null, firstPrompt: '{"role":"agent"}' })).toBe(
      "Session in worktree",
    );
  });
  it("uses project context to identify sessions with no readable title or prompt", () => {
    expect(
      sessionTitle(
        { ...main, title: null, firstPrompt: "<session metadata>", cwd: "/repo/OpenCovibe" },
        (project) => `In ${project}`,
      ),
    ).toBe("In OpenCovibe");
  });
  it("filters known helper sessions by default and exposes them with the include option", () => {
    const memory = { ...main, sessionId: "memory", filePath: "/.claude-mem/session.jsonl" };
    const observer = {
      ...main,
      sessionId: "observer",
      filePath: "/observer-sessions/observer.jsonl",
    };
    expect(isBackgroundSession(memory)).toBe(true);
    expect(isBackgroundSession(observer)).toBe(true);
    expect(filterSessions([main, memory, observer], "", false, false)).toEqual([main]);
    expect(filterSessions([main, memory, observer], "", false, false, true)).toHaveLength(3);
  });
  it("does not show protocol-shaped first text as a preview", () => {
    expect(sessionPreview({ ...main, firstPrompt: '{"type":"observer"}' })).toBe("");
  });
});

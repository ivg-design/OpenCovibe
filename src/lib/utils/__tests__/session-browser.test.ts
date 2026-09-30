import { describe, it, expect } from "vitest";
import type { CliSessionSummary } from "$lib/types";
import { filterSessions, sessionTitle, sessionProject, sessionPreview } from "../session-browser";

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
      "Untitled conversation",
    );
  });
});

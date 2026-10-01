import { describe, expect, it } from "vitest";
import { resolveRoomMentionIds } from "../room-mentions";

const participants = [
  { id: "lead", name: "Lead" },
  { id: "claude", name: "Claude delegate" },
  { id: "codex", name: "Codex delegate" },
];

describe("room composer mention recipients", () => {
  it("resolves names with spaces and multiple explicit recipients", () => {
    expect(resolveRoomMentionIds("@lead and @Claude delegate: review", participants)).toEqual([
      "lead",
      "claude",
    ]);
  });

  it("ignores email addresses, links, inline code, and fenced code like the backend", () => {
    for (const body of ["hello@Lead", "https://example.com/@Lead", "`@Lead`", "```\n@Lead\n```"]) {
      expect(resolveRoomMentionIds(body, participants)).toBeNull();
    }
  });

  it("treats @everyone as the recipient override", () => {
    expect(resolveRoomMentionIds("@Lead @everyone", participants)).toEqual([]);
  });
});

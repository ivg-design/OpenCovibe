import { describe, expect, it } from "vitest";
import { isRequestReplyShortcut, requestReplyApproval } from "./request-reply";
import type { RoomRequest } from "./types";

const room = { archived: false, paused: true, participants: [] };
const request = (kind: RoomRequest["kind"], status: RoomRequest["status"]): RoomRequest => ({
  id: "request",
  kind,
  status,
  requester_id: "lead",
  reviewer_id: null,
  title: "Response",
  body: "Details",
  evidence: null,
  task_id: null,
  proposal: null,
  brief: null,
  options: [],
  response: null,
  resolved_by: null,
  approved_participant_id: null,
  work_signature: null,
  created_at: "",
  updated_at: "",
});

describe("request response keyboard submission", () => {
  it("submits the field's enabled action while retaining completion review and pause gates", () => {
    expect(requestReplyApproval(room, request("decision", "pending"), "Answer", false)).toBe(true);
    expect(requestReplyApproval(room, request("completion", "verified"), "Accepted", false)).toBe(
      true,
    );
    expect(
      requestReplyApproval(
        { ...room, paused: false },
        request("completion", "verified"),
        "Accepted",
        false,
      ),
    ).toBeNull();
    expect(requestReplyApproval(room, request("completion", "pending"), "Rejected", false)).toBe(
      false,
    );
    expect(requestReplyApproval(room, request("review", "pending"), "Cancelled", false)).toBe(
      false,
    );
    expect(requestReplyApproval(room, request("agent", "pending"), "Rejected", false)).toBe(false);
    expect(
      requestReplyApproval(
        room,
        { ...request("agent", "pending"), approved_participant_id: "peer" },
        "Rejected",
        false,
      ),
    ).toBeNull();
    for (const [r, text, disabled] of [
      [request("decision", "approved"), "Answer", false],
      [request("decision", "pending"), " ", false],
      [request("decision", "pending"), "Answer", true],
      [{ ...request("decision", "pending"), archived: true }, "Answer", false],
    ] as const)
      expect(requestReplyApproval(room, r, text, disabled)).toBeNull();
    expect(
      requestReplyApproval(
        { ...room, archived: true },
        request("decision", "pending"),
        "Answer",
        false,
      ),
    ).toBeNull();
  });
  it("uses Cmd/Ctrl+Enter and leaves multiline, composition and repeats alone", () => {
    const event = {
      key: "Enter",
      metaKey: true,
      ctrlKey: false,
      altKey: false,
      shiftKey: false,
      isComposing: false,
      repeat: false,
    };
    expect(isRequestReplyShortcut(event)).toBe(true);
    expect(isRequestReplyShortcut({ ...event, metaKey: false, ctrlKey: true })).toBe(true);
    for (const patch of [
      { metaKey: false },
      { shiftKey: true },
      { altKey: true },
      { isComposing: true },
      { repeat: true },
      { key: "Escape" },
    ])
      expect(isRequestReplyShortcut({ ...event, ...patch })).toBe(false);
  });
});

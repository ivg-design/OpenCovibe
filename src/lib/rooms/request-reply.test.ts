import { describe, expect, it } from "vitest";
import {
  completionApprovalBlocker,
  isRequestReplyShortcut,
  requestReplyApproval,
} from "./request-reply";
import type { RoomParticipant, RoomRequest } from "./types";

const room = { archived: false, paused: true, participants: [] };
const participant = (
  state: string,
  pending_delivery: RoomParticipant["pending_delivery"] = null,
): RoomParticipant => ({
  id: "peer",
  name: "Peer",
  provider: "codex",
  run_id: "run",
  paused: false,
  model: null,
  effort: null,
  worktree_path: null,
  branch: null,
  state,
  last_error: null,
  last_wake_at: null,
  wake_count: 0,
  max_turns: 0,
  event_cursor: 0,
  message_cursor: 0,
  pending_delivery,
  no_progress_turns: 0,
  work_signature: null,
});
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

describe("completion approval gates", () => {
  const verifiedCompletion = request("completion", "verified");

  it("explains each disabled state and enables only when all approval gates pass", () => {
    expect(completionApprovalBlocker(room, verifiedCompletion, "Accepted", true)).toBe("busy");
    expect(
      completionApprovalBlocker({ ...room, paused: false }, verifiedCompletion, "Accepted", false),
    ).toBe("pause");
    expect(completionApprovalBlocker(room, verifiedCompletion, "  ", false)).toBe("note");
    expect(completionApprovalBlocker(room, verifiedCompletion, "Accepted", false)).toBeNull();
    expect(
      completionApprovalBlocker({ ...room, archived: true }, verifiedCompletion, "Accepted", false),
    ).toBe("unavailable");
    expect(
      completionApprovalBlocker(room, { ...verifiedCompletion, archived: true }, "Accepted", false),
    ).toBe("unavailable");
  });

  it("keeps active, busy, pending, running, waiting, and pending-delivery peers from being approved", () => {
    for (const state of ["active", "busy", "pending", "running", "waiting"])
      expect(
        completionApprovalBlocker(
          { ...room, participants: [participant(state)] },
          verifiedCompletion,
          "Accepted",
          false,
        ),
      ).toBe("participants");
    expect(
      completionApprovalBlocker(
        {
          ...room,
          participants: [
            participant("idle", {
              id: "delivery",
              reason: "turn",
              text: "message",
              created_at: 0,
              state: "pending",
              task_id: null,
              timer_id: null,
            }),
          ],
        },
        verifiedCompletion,
        "Accepted",
        false,
      ),
    ).toBe("participants");
  });

  it("uses the same completion gates for Cmd/Ctrl+Enter", () => {
    expect(requestReplyApproval(room, verifiedCompletion, "Accepted", false)).toBe(true);
    for (const blockedRoom of [
      { ...room, paused: false },
      { ...room, participants: [participant("busy")] },
      {
        ...room,
        participants: [
          participant("idle", {
            id: "delivery",
            reason: "turn",
            text: "message",
            created_at: 0,
            state: "pending",
            task_id: null,
            timer_id: null,
          }),
        ],
      },
    ])
      expect(requestReplyApproval(blockedRoom, verifiedCompletion, "Accepted", false)).toBeNull();
  });
});

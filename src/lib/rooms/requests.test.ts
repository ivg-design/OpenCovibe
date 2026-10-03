import { describe, expect, it } from "vitest";
import {
  filterRoomRequests,
  isOpenRoomRequest,
  needsHumanAnswer,
  requestInboxKeys,
  hasNewRequest,
  requestPanelWidth,
} from "./requests";
import type { RoomParticipant, RoomRequest } from "./types";
const request = (
  id: string,
  kind: RoomRequest["kind"],
  status: RoomRequest["status"],
): RoomRequest => ({
  id,
  kind,
  status,
  requester_id: "lead",
  reviewer_id: null,
  title: id,
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
  created_at: "2026-10-02T10:00:00Z",
  updated_at: "2026-10-02T10:00:00Z",
});
describe("room request inbox", () => {
  it("separates waiting, human attention, resolved and archived requests by type", () => {
    const decision = request("question", "decision", "pending");
    const review = request("review", "review", "pending");
    const closed = request("obsolete", "decision", "closed");
    const archived = { ...request("old", "review", "approved"), archived: true };
    const requests = [decision, review, closed, archived];
    expect(filterRoomRequests(requests, "attention", "", [])).toEqual([decision]);
    expect(filterRoomRequests(requests, "waiting", "", [])).toEqual([review]);
    expect(filterRoomRequests(requests, "history", "", [])).toEqual([closed]);
    expect(filterRoomRequests(requests, "archived", "", [])).toEqual([archived]);
    expect(filterRoomRequests(requests, "all", "", [], "review")).toEqual([review]);
  });
  it("reopens for new actionable work, not unchanged polls or completed work", () => {
    const pending = request("a", "decision", "pending");
    const known = requestInboxKeys([pending]);
    expect(hasNewRequest(known, requestInboxKeys([{ ...pending }]))).toBe(false);
    expect(
      hasNewRequest(known, requestInboxKeys([pending, request("b", "review", "pending")])),
    ).toBe(false);
    expect(hasNewRequest(known, requestInboxKeys([{ ...pending, status: "approved" }]))).toBe(
      false,
    );
    expect(
      hasNewRequest(["c:pending"], requestInboxKeys([request("c", "completion", "verified")])),
    ).toBe(true);
    expect(requestPanelWidth(NaN)).toBe(32);
    expect(requestPanelWidth(90)).toBe(70);
    expect(requestPanelWidth(2)).toBe(20);
  });
  it("ignores peer arrivals and review updates until they require a human answer", () => {
    const known = requestInboxKeys([request("question", "decision", "pending")]);
    for (const peer of [
      request("peer", "review", "pending"),
      request("peer", "review", "changes_requested"),
      request("proposal", "completion", "pending"),
    ]) {
      expect(
        hasNewRequest(known, requestInboxKeys([request("question", "decision", "pending"), peer])),
      ).toBe(false);
      expect(filterRoomRequests([peer], "attention", "", [])).toEqual([]);
      expect(filterRoomRequests([peer], "waiting", "", [])).toEqual([peer]);
    }
    expect(
      hasNewRequest(known, requestInboxKeys([request("proposal", "completion", "verified")])),
    ).toBe(true);
    expect(
      hasNewRequest(known, requestInboxKeys([request("new-question", "decision", "pending")])),
    ).toBe(true);
    expect(
      requestInboxKeys([{ ...request("archived", "decision", "pending"), archived: true }]),
    ).toEqual([]);
  });
  it("distinguishes human decisions from pending peer reviews and closed requests", () => {
    expect(needsHumanAnswer(request("a", "decision", "pending"))).toBe(true);
    expect(needsHumanAnswer(request("b", "agent", "pending"))).toBe(true);
    expect(needsHumanAnswer(request("c", "completion", "verified"))).toBe(true);
    for (const [kind, status] of [
      ["review", "pending"],
      ["review", "changes_requested"],
      ["completion", "pending"],
      ["decision", "approved"],
      ["agent", "creating"],
    ] as const) {
      expect(needsHumanAnswer(request("x", kind, status))).toBe(false);
    }
  });
  it("prioritizes human answers while keeping open peer work and searchable history", () => {
    const review = request("new peer review", "review", "pending");
    review.updated_at = "2026-10-02T11:00:00Z";
    const answer = request("older decision", "decision", "pending");
    const done = request("closed", "decision", "approved");
    done.response = "Use the replay approach";
    const requests = [review, answer, done];
    expect(filterRoomRequests(requests, "open", "", []).map((r) => r.id)).toEqual([
      answer.id,
      review.id,
    ]);
    expect(filterRoomRequests(requests, "history", "REPLAY", []).map((r) => r.id)).toEqual([
      done.id,
    ]);
    expect(filterRoomRequests(requests, "all", "unknown", [])).toEqual([]);
    expect(isOpenRoomRequest(request("review", "completion", "verified"))).toBe(true);
    expect(isOpenRoomRequest(done)).toBe(false);
  });
  it("finds requests by assigned reviewer, evidence or review response", () => {
    const review = request("Review", "review", "changes_requested");
    review.reviewer_id = "claude";
    review.evidence = "Commit abc123";
    review.review_response = "Fix stale selection";
    const peers = [{ id: "claude", name: "Claude reviewer" }] as RoomParticipant[];
    for (const term of ["claude reviewer", "abc123", "stale selection"])
      expect(filterRoomRequests([review], "all", term, peers)).toEqual([review]);
  });
});

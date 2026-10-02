import type { RoomParticipant, RoomRequest } from "./types";

export function isOpenRoomRequest(request: RoomRequest): boolean {
  return ["pending", "creating", "changes_requested", "verified"].includes(request.status);
}

export function needsHumanAnswer(request: RoomRequest): boolean {
  return (
    (request.status === "pending" && ["agent", "decision"].includes(request.kind)) ||
    (request.kind === "completion" && request.status === "verified")
  );
}

export function filterRoomRequests(
  requests: RoomRequest[],
  scope: "open" | "history" | "all",
  query: string,
  participants: RoomParticipant[],
): RoomRequest[] {
  const term = query.trim().toLocaleLowerCase();
  const name = (id: string | null) => participants.find((p) => p.id === id)?.name ?? "";
  return requests
    .filter(
      (request) =>
        (scope === "all" || isOpenRoomRequest(request) === (scope === "open")) &&
        (!term ||
          [
            request.title,
            request.body,
            request.evidence,
            request.response,
            request.review_response,
            name(request.requester_id),
            name(request.reviewer_id),
          ].some((value) => value?.toLocaleLowerCase().includes(term))),
    )
    .sort(
      (a, b) =>
        Number(needsHumanAnswer(b)) - Number(needsHumanAnswer(a)) ||
        b.updated_at.localeCompare(a.updated_at),
    );
}

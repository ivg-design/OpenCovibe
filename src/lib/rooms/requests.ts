import type { RoomParticipant, RoomRequest } from "./types";

export function requestInboxKeys(requests: RoomRequest[]): string[] {
  return requests.filter(isOpenRoomRequest).map((r) => `${r.id}:${r.status}`);
}
export function hasNewRequest(previous: string[], next: string[]): boolean {
  const known = new Set(previous);
  return next.some((key) => !known.has(key));
}
export function requestPanelWidth(value: number): number {
  return Number.isFinite(value) ? Math.max(20, Math.min(70, value)) : 32;
}

export function isOpenRoomRequest(request: RoomRequest): boolean {
  return (
    !request.archived &&
    ["pending", "creating", "changes_requested", "verified"].includes(request.status)
  );
}

export function needsHumanAnswer(request: RoomRequest): boolean {
  return (
    !request.archived &&
    ((request.status === "pending" && ["agent", "decision"].includes(request.kind)) ||
      (request.kind === "completion" && request.status === "verified"))
  );
}

export function filterRoomRequests(
  requests: RoomRequest[],
  scope: "open" | "attention" | "waiting" | "history" | "archived" | "all",
  query: string,
  participants: RoomParticipant[],
  kind = "all",
): RoomRequest[] {
  const term = query.trim().toLocaleLowerCase();
  const name = (id: string | null) => participants.find((p) => p.id === id)?.name ?? "";
  return requests
    .filter(
      (request) =>
        (scope === "archived"
          ? !!request.archived
          : !request.archived &&
            (scope === "all" ||
              (scope === "attention"
                ? needsHumanAnswer(request)
                : scope === "waiting"
                  ? isOpenRoomRequest(request) && !needsHumanAnswer(request)
                  : isOpenRoomRequest(request) === (scope === "open")))) &&
        (kind === "all" || request.kind === kind) &&
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

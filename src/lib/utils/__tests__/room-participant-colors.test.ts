import { describe, expect, it } from "vitest";
import { roomParticipantColor, roomParticipantColorAt } from "../room-participant-colors";

describe("room participant colors", () => {
  it("uses the participant order shared by room messages and sidebar rows", () => {
    expect(roomParticipantColorAt(1, "peer-b")).toBe(roomParticipantColorAt(1, "peer-b"));
    expect(roomParticipantColorAt(1, "peer-b")).not.toBe(roomParticipantColorAt(2, "peer-c"));
  });

  it("maps a room participant id to its room-order color", () => {
    const participants = [{ id: "a" }, { id: "b" }] as Parameters<typeof roomParticipantColor>[0];
    expect(roomParticipantColor(participants, "b")).toBe(roomParticipantColorAt(1, "b"));
  });
});

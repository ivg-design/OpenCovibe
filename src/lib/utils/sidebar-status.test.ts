import { describe, expect, it } from "vitest";
import type { RoomSidebarEntry } from "$lib/rooms/types";
import { sidebarAgentStatus, sidebarRoomState } from "./sidebar-status";

function room(states: string[], overrides: Partial<RoomSidebarEntry> = {}): RoomSidebarEntry {
  return {
    id: "room",
    title: "Test room",
    repo_path: "/repo",
    updated_at: "",
    needs_answer: 0,
    participants: states.map((state, index) => ({
      state,
      run_id: `run-${index}`,
      participant_id: `peer-${index}`,
      name: `Agent ${index}`,
      provider: "codex",
      color_index: index,
    })),
    ...overrides,
  };
}

describe("sidebar status", () => {
  it("shows activity instead of calling idle agents done", () => {
    expect(sidebarAgentStatus("running")).toEqual({ status: "running", label: "room_stateBusy" });
    expect(sidebarAgentStatus("busy")).toEqual(sidebarAgentStatus("running"));
    expect(sidebarAgentStatus("idle")).toEqual({ status: "idle", label: "room_stateIdle" });
    expect(sidebarAgentStatus("completed")).toEqual(sidebarAgentStatus("idle"));
    expect(sidebarAgentStatus("running", true).label).toBe("room_stateWaiting");
    expect(sidebarAgentStatus("budget_exhausted").label).toBe("room_statePaused");
    expect(sidebarAgentStatus("failed").status).toBe("failed");
    expect(sidebarAgentStatus("unrecognized").label).toBe("room_stateUnknown");
  });
  it("shows the room pause gate even when participants are unpaused and idle", () => {
    expect(sidebarRoomState(room(["idle", "busy"], { paused: true }))).toBe("paused");
    expect(sidebarRoomState(room(["idle", "busy"]))).toBe("busy");
    expect(sidebarRoomState(room(["paused", "idle"]))).toBe("idle");
    expect(sidebarRoomState(room(["paused", "no_progress"]))).toBe("paused");
  });
  it("keeps requests and failures visible in a quiescent room", () => {
    expect(sidebarRoomState(room(["idle"], { needs_answer: 1 }))).toBe("waiting");
    expect(sidebarRoomState(room(["idle", "failed"]))).toBe("failed");
    expect(sidebarRoomState(room(["idle", "blocked"]))).toBe("blocked");
    expect(sidebarRoomState(room(["spawning", "idle"]))).toBe("starting");
    expect(sidebarRoomState(room([]))).toBe("idle");
  });
});

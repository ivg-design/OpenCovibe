import { describe, expect, it, vi, afterEach } from "vitest";
import { revealRoom, visibleRooms, visibleRoomRuns } from "./visibility";

afterEach(() => vi.unstubAllGlobals());
describe("room visibility", () => {
  const room = { id: "new", repo_path: "/repo" };
  it("does not turn hidden room members or repeated imports into standalone agents", () => {
    const runs = [
      { id: "old-peer", session_id: "saved" },
      { id: "alias", session_id: "saved" },
      { id: "new-peer" },
      { id: "solo" },
    ];
    expect(
      visibleRoomRuns(
        runs,
        [
          { ...room, id: "old", participants: [{ run_id: "old-peer" }] },
          { ...room, participants: [{ run_id: "new-peer" }] },
        ],
        [room],
      ),
    ).toEqual(runs.slice(2));
  });
  it("uses the same active list for removed folders, hidden rooms and archives", () => {
    expect(
      visibleRooms(
        [room, { ...room, id: "old" }, { ...room, id: "archive", archived: true }],
        [],
        ["old"],
      ),
    ).toEqual([room]);
    expect(visibleRooms([room], ["/repo/"], [])).toEqual([]);
  });
  it("reveals a new room while keeping older removed rooms hidden, including after reload", () => {
    const values = new Map<string, string>([["ocv:removed-cwds", '["/repo","/other"]']]);
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => values.set(key, value),
    });
    vi.stubGlobal("window", { dispatchEvent: vi.fn() });
    const rooms = [room, { ...room, id: "old" }, { id: "other", repo_path: "/other" }];
    revealRoom(room, rooms);
    expect(visibleRooms(rooms)).toEqual([room]);
    expect(JSON.parse(values.get("ocv:removed-cwds")!)).toEqual(["/other"]);
    revealRoom(rooms[1], rooms);
    expect(visibleRooms(rooms)).toEqual(rooms.slice(0, 2));
  });
});

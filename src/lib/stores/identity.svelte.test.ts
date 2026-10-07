import { afterEach, describe, expect, it } from "vitest";
import { identityName, roomSenderName, setIdentityName } from "./identity.svelte";

afterEach(() => setIdentityName(null));
describe("display identity", () => {
  it("updates existing human message labels while preserving agent names and role IDs", () => {
    const message = { sender: "Human", participant_id: null };
    setIdentityName(" Ilya ");
    expect(identityName()).toBe("Ilya");
    expect(roomSenderName(message)).toBe("Ilya");
    expect(message.sender).toBe("Human");
    expect(roomSenderName({ sender: "Human", participant_id: "agent-id" })).toBe("Human");
    expect(roomSenderName({ sender: "Claude", participant_id: "claude-id" })).toBe("Claude");
    setIdentityName("Nemo owner");
    expect(roomSenderName(message)).toBe("Nemo owner");
  });
  it("restores the default when the name is cleared or missing in legacy settings", () => {
    for (const name of [null, undefined, "", "  "]) {
      setIdentityName(name);
      expect(identityName()).toBe("Human");
    }
  });
});

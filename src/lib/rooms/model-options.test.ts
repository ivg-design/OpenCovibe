import { describe, expect, it } from "vitest";
import { roomEffortLevels, effortAfterModelChange } from "./model-options";
import type { CliModelInfo } from "$lib/types";

const models: CliModelInfo[] = [
  {
    value: "new-account-model",
    displayName: "New account model",
    description: "",
    supportsEffort: true,
    supportedEffortLevels: ["low", "high", "future-effort"],
  },
  { value: "small", displayName: "Small", description: "", supportsEffort: false },
  {
    value: "default",
    displayName: "Default",
    description: "",
    supportsEffort: true,
    supportedEffortLevels: ["medium", "max"],
  },
];
describe("room model capabilities", () => {
  it("uses the selected account model's capabilities, including new effort choices", () => {
    expect(roomEffortLevels(models, "new-account-model")).toEqual(["low", "high", "future-effort"]);
    expect(roomEffortLevels(models, "small")).toEqual([]);
  });
  it("uses the discovered provider default rather than another model's effort list", () => {
    expect(roomEffortLevels(models, "", "new-account-model")).toEqual([
      "low",
      "high",
      "future-effort",
    ]);
    expect(roomEffortLevels(models, "")).toEqual(["medium", "max"]);
  });
  it("does not invent capabilities when discovery is unavailable or the model is unknown", () => {
    expect(roomEffortLevels([], "new-account-model")).toEqual([]);
    expect(roomEffortLevels(models, "old-imported-model")).toEqual([]);
  });
  it("clears incompatible effort only when the user switches models", () => {
    expect(effortAfterModelChange(models, "new-account-model", "high")).toBe("high");
    expect(effortAfterModelChange(models, "new-account-model", "max")).toBe("");
    expect(effortAfterModelChange(models, "small", "high")).toBe("");
  });
});

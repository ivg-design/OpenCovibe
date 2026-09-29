import { describe, expect, it } from "vitest";
import {
  CUSTOM_PALETTE_STORAGE_KEY,
  contrastRatio,
  hslChannelsToHex,
  hexToHslChannels,
  isHexColor,
  normalizePalette,
  primaryForeground,
  readCustomPalette,
  saveCustomPalette,
  type PaletteStorage,
} from "./custom-palette";

function memoryStorage(): PaletteStorage & { values: Map<string, string> } {
  const values = new Map<string, string>();
  return {
    values,
    getItem: (key) => values.get(key) ?? null,
    setItem: (key, value) => void values.set(key, value),
    removeItem: (key) => void values.delete(key),
  };
}

describe("custom palette", () => {
  it("accepts only six-digit hex and drops unknown or invalid stored values", () => {
    expect(isHexColor("#12aBcF")).toBe(true);
    expect(isHexColor("#abc")).toBe(false);
    expect(isHexColor("red;--x:1")).toBe(false);
    expect(normalizePalette({ primary: "#12abCf", injected: "#ffffff", border: "#bad" })).toEqual({
      primary: "#12ABCF",
    });
  });

  it("persists validated palette values and removes storage on reset", () => {
    const storage = memoryStorage();
    expect(saveCustomPalette(storage, { primary: "#e9b85f", border: "invalid" })).toBe(true);
    expect(readCustomPalette(storage)).toEqual({ primary: "#E9B85F" });
    expect(storage.values.has(CUSTOM_PALETTE_STORAGE_KEY)).toBe(true);
    expect(saveCustomPalette(storage, {})).toBe(true);
    expect(readCustomPalette(storage)).toEqual({});
  });

  it("handles inaccessible or malformed storage without throwing", () => {
    const blocked: PaletteStorage = {
      getItem: () => {
        throw new Error("blocked");
      },
      setItem: () => {
        throw new Error("blocked");
      },
      removeItem: () => {
        throw new Error("blocked");
      },
    };
    expect(readCustomPalette(blocked)).toEqual({});
    expect(saveCustomPalette(blocked, { primary: "#ffffff" })).toBe(false);
    const malformed = memoryStorage();
    malformed.values.set(CUSTOM_PALETTE_STORAGE_KEY, "{");
    expect(readCustomPalette(malformed)).toEqual({});
  });

  it("converts colors for CSS tokens and chooses readable primary text", () => {
    expect(hexToHslChannels("#FF0000")).toBe("0 100% 50%");
    expect(hslChannelsToHex("0 100% 50%")).toBe("#FF0000");
    expect(hslChannelsToHex(hexToHslChannels("#E9B85F")!)).toBe("#E9B85F");
    expect(hslChannelsToHex("invalid")).toBeNull();
    expect(contrastRatio("#000000", "#FFFFFF")).toBeCloseTo(21);
    expect(primaryForeground("#000000")).toBe("#FFFFFF");
    expect(primaryForeground("#FFFFFF")).toBe("#000000");
    expect(contrastRatio("bad", "#ffffff")).toBeNull();
  });
});

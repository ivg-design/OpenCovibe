export const CUSTOM_PALETTE_STORAGE_KEY = "ocv:custom-palette";

export const PALETTE_FIELDS = ["primary", "background", "sidebar", "foreground", "border"] as const;
export type PaletteField = (typeof PALETTE_FIELDS)[number];
export type CustomPalette = Partial<Record<PaletteField, string>>;

export interface PaletteStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

export const HEX_COLOR_PATTERN = /^#[\da-fA-F]{6}$/;

export function isHexColor(value: string): boolean {
  return HEX_COLOR_PATTERN.test(value);
}

export function normalizePalette(value: unknown): CustomPalette {
  if (!value || typeof value !== "object" || Array.isArray(value)) return {};
  const palette: CustomPalette = {};
  for (const field of PALETTE_FIELDS) {
    const color = (value as Record<string, unknown>)[field];
    if (typeof color === "string" && isHexColor(color)) palette[field] = color.toUpperCase();
  }
  return palette;
}

export function readCustomPalette(storage: PaletteStorage | null): CustomPalette {
  if (!storage) return {};
  try {
    const stored = storage.getItem(CUSTOM_PALETTE_STORAGE_KEY);
    return stored ? normalizePalette(JSON.parse(stored)) : {};
  } catch {
    return {};
  }
}

export function saveCustomPalette(storage: PaletteStorage | null, palette: CustomPalette): boolean {
  if (!storage) return false;
  const safePalette = normalizePalette(palette);
  try {
    if (Object.keys(safePalette).length === 0) storage.removeItem(CUSTOM_PALETTE_STORAGE_KEY);
    else storage.setItem(CUSTOM_PALETTE_STORAGE_KEY, JSON.stringify(safePalette));
    return true;
  } catch {
    return false;
  }
}

export function hexToHslChannels(hex: string): string | null {
  if (!isHexColor(hex)) return null;
  const channels = [1, 3, 5].map((offset) => parseInt(hex.slice(offset, offset + 2), 16) / 255);
  const [red, green, blue] = channels;
  const max = Math.max(red, green, blue);
  const min = Math.min(red, green, blue);
  const delta = max - min;
  let hue = 0;
  let saturation = 0;
  const lightness = (max + min) / 2;
  if (delta !== 0) {
    saturation = delta / (1 - Math.abs(2 * lightness - 1));
    if (max === red) hue = ((green - blue) / delta) % 6;
    else if (max === green) hue = (blue - red) / delta + 2;
    else hue = (red - green) / delta + 4;
    hue = (hue * 60 + 360) % 360;
  }
  return `${Number(hue.toFixed(4))} ${Number((saturation * 100).toFixed(4))}% ${Number((lightness * 100).toFixed(4))}%`;
}

export function hslChannelsToHex(channels: string): string | null {
  const match = channels.trim().match(/^(-?\d+(?:\.\d+)?)\s+(\d+(?:\.\d+)?)%\s+(\d+(?:\.\d+)?)%$/);
  if (!match) return null;
  const hue = ((Number(match[1]) % 360) + 360) % 360;
  const saturation = Number(match[2]) / 100;
  const lightness = Number(match[3]) / 100;
  if (saturation > 1 || lightness > 1) return null;

  const chroma = (1 - Math.abs(2 * lightness - 1)) * saturation;
  const second = chroma * (1 - Math.abs(((hue / 60) % 2) - 1));
  const offset = lightness - chroma / 2;
  const rgb =
    hue < 60
      ? [chroma, second, 0]
      : hue < 120
        ? [second, chroma, 0]
        : hue < 180
          ? [0, chroma, second]
          : hue < 240
            ? [0, second, chroma]
            : hue < 300
              ? [second, 0, chroma]
              : [chroma, 0, second];
  return `#${rgb
    .map((channel) =>
      Math.round((channel + offset) * 255)
        .toString(16)
        .padStart(2, "0"),
    )
    .join("")}`.toUpperCase();
}

function linearize(channel: number): number {
  const value = channel / 255;
  return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
}

export function contrastRatio(hexA: string, hexB: string): number | null {
  if (!isHexColor(hexA) || !isHexColor(hexB)) return null;
  const luminance = (hex: string) => {
    const values = [1, 3, 5].map((offset) =>
      linearize(parseInt(hex.slice(offset, offset + 2), 16)),
    );
    return 0.2126 * values[0] + 0.7152 * values[1] + 0.0722 * values[2];
  };
  const first = luminance(hexA);
  const second = luminance(hexB);
  return (Math.max(first, second) + 0.05) / (Math.min(first, second) + 0.05);
}

export function primaryForeground(hex: string): "#000000" | "#FFFFFF" | null {
  if (!isHexColor(hex)) return null;
  return contrastRatio(hex, "#000000")! >= contrastRatio(hex, "#FFFFFF")! ? "#000000" : "#FFFFFF";
}

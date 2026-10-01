/** Values from a browser drag payload that should become readable chat text. */
export function normalizeDroppedLinks(value: string): string[] {
  return [
    ...new Set(
      value
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter((line) => line && !line.startsWith("#"))
        .map((line) => {
          try {
            const url = new URL(line);
            return url.protocol === "http:" || url.protocol === "https:" ? url.href : "";
          } catch {
            return "";
          }
        })
        .filter(Boolean),
    ),
  ];
}

/** De-duplicate identical clipboard/drop files without conflating separate same-name files. */
export function uniqueComposerFiles(files: Iterable<File>): File[] {
  const seen = new Set<string>();
  const result: File[] = [];
  for (const file of files) {
    const key = `${file.name}\u0000${file.type}\u0000${file.size}\u0000${file.lastModified}`;
    if (seen.has(key)) continue;
    seen.add(key);
    result.push(file);
  }
  return result;
}

export const ROOM_ATTACHMENT_MAX_COUNT = 8;
export const ROOM_ATTACHMENT_MAX_FILE_BYTES = 20 * 1024 * 1024;
export const ROOM_ATTACHMENT_MAX_TOTAL_BYTES = 80 * 1024 * 1024;

export function fitRoomAttachmentLimits<T extends { size: number }>(
  existing: readonly { size: number }[],
  candidates: readonly T[],
): { accepted: T[]; rejected: T[] } {
  let count = existing.length;
  let totalBytes = existing.reduce((sum, item) => sum + item.size, 0);
  const accepted: T[] = [];
  const rejected: T[] = [];
  for (const candidate of candidates) {
    if (
      candidate.size > ROOM_ATTACHMENT_MAX_FILE_BYTES ||
      count >= ROOM_ATTACHMENT_MAX_COUNT ||
      totalBytes + candidate.size > ROOM_ATTACHMENT_MAX_TOTAL_BYTES
    ) {
      rejected.push(candidate);
      continue;
    }
    accepted.push(candidate);
    count++;
    totalBytes += candidate.size;
  }
  return { accepted, rejected };
}

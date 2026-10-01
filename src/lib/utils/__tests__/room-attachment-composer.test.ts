import { describe, expect, it } from "vitest";
import {
  fitRoomAttachmentLimits,
  normalizeDroppedLinks,
  ROOM_ATTACHMENT_MAX_FILE_BYTES,
  ROOM_ATTACHMENT_MAX_TOTAL_BYTES,
  uniqueComposerFiles,
} from "../room-attachment-composer";

describe("room attachment composer helpers", () => {
  it("normalizes HTTP(S) links from uri-list and drops comments or unsafe schemes", () => {
    expect(
      normalizeDroppedLinks(
        "# browser comment\r\nhttps://example.com/a\n javascript:alert(1)\nhttp://example.org",
      ),
    ).toEqual(["https://example.com/a", "http://example.org/"]);
  });

  it("de-duplicates files repeated by overlapping paste and drop sources", () => {
    const file = new File(["same"], "image.png", { type: "image/png", lastModified: 1 });
    expect(uniqueComposerFiles([file, file])).toEqual([file]);
  });

  it("enforces the combined count, per-file, and total-size room limits", () => {
    const result = fitRoomAttachmentLimits(
      [{ size: ROOM_ATTACHMENT_MAX_TOTAL_BYTES - 5 }],
      [{ size: 4 }, { size: 2 }, { size: ROOM_ATTACHMENT_MAX_FILE_BYTES + 1 }],
    );
    expect(result.accepted).toEqual([{ size: 4 }]);
    expect(result.rejected).toHaveLength(2);
  });
});

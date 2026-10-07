import type { RoomParticipant } from "$lib/rooms/types";

/** Match the room backend mention rules so the composer recipient hint stays accurate. */
export function resolveRoomMentionIds(
  body: string,
  participants: readonly Pick<RoomParticipant, "id" | "name">[],
): string[] | null {
  const candidates = [...participants].sort((a, b) => b.name.length - a.name.length);
  const ids: string[] = [];
  let fenced = false;
  let inline = false;
  for (const line of body.split(/\r?\n/)) {
    if (/^\s*(?:```|~~~)/.test(line)) {
      fenced = !fenced;
      continue;
    }
    if (fenced) continue;
    for (let index = 0; index < line.length; index++) {
      const character = line[index];
      if (character === "`") inline = !inline;
      if (inline || character !== "@") continue;
      const previous = [...line.slice(0, index)].at(-1);
      if (previous && !/\s/.test(previous) && !/[([{"',]/.test(previous)) continue;
      const rest = line.slice(index + 1);
      const findMatch = (name: string) => {
        const prefix = Array.from(rest).slice(0, Array.from(name).length).join("");
        if (prefix.toLocaleLowerCase() !== name.toLocaleLowerCase()) return false;
        const next = Array.from(rest.slice(prefix.length))[0];
        return !next || !/[\p{L}\p{N}_-]/u.test(next);
      };
      if (findMatch("everyone")) return [];
      const participant = candidates.find((candidate) => findMatch(candidate.name));
      if (participant && !ids.includes(participant.id)) ids.push(participant.id);
    }
    inline = false;
  }
  return ids.length ? ids : null;
}

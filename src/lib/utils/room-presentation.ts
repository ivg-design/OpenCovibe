export interface RoomBriefing {
  participant: string;
  objective: string;
  reason: string;
}

/** Recognize only host-generated room briefings, including old saved transcripts. */
export function roomBriefing(text: string): RoomBriefing | null {
  if (text.trimStart().startsWith("<heartbeat>")) {
    const instructions = /<instructions>\s*([\s\S]*?)(?:<\/instructions>|$)/.exec(text)?.[1];
    return {
      participant: "",
      objective: instructions?.trim().slice(0, 600) ?? "Scheduled follow-up",
      reason: "timer",
    };
  }
  if (text.trimStart().startsWith("<codex_delegation>")) {
    const input = /<input>\s*([\s\S]*?)(?:<\/input>|$)/.exec(text)?.[1];
    return {
      participant: "",
      objective: input?.trim().slice(0, 600) ?? "Agent update",
      reason: "message",
    };
  }
  const match =
    /^You are (.+?) \([^\n)]+\) in a persistent peer room\. Global objective: ([\s\S]*?)(?:\nApproved participant brief:|$)/.exec(
      text,
    );
  if (!match) return null;
  return {
    participant: match[1],
    objective: match[2].trim().slice(0, 600),
    reason: /^Wake reason: (.+)$/m.exec(text)?.[1] ?? "check-in",
  };
}

export function roomChatTitle(text: string, prompt = text): string {
  const source = text.startsWith("You are ") ? prompt : text;
  const briefing = roomBriefing(source);
  if (briefing) return briefing.participant || "Scheduled follow-up";
  const initial = /^You are a participant in the room '(.+?)'\. Objective:/.exec(source);
  return initial ? initial[1] : text;
}

/** Human-readable presentation of protocol objects; never expose serialized schemas. */
export function readableProtocolData(value: unknown, depth = 0): string {
  if (value == null) return "";
  if (typeof value === "boolean") return value ? "Yes" : "No";
  if (typeof value !== "object") {
    const text = String(value);
    const briefing = roomBriefing(text.replace(/^Original request: /, ""));
    if (briefing) return briefing.objective;
    if (/^[[{]/.test(text.trim())) {
      try {
        return readableProtocolData(JSON.parse(text), depth);
      } catch {
        return /^\s*(?:\{\s*"|\[\s*[{"])/.test(text)
          ? "Structured information received."
          : text.slice(0, 1500);
      }
    }
    return text.slice(0, 1500);
  }
  if (depth > 3) return "Additional details available in the shared board.";
  if (Array.isArray(value)) {
    return (
      value
        .slice(0, 12)
        .map((item) => readableProtocolData(item, depth + 1))
        .filter(Boolean)
        .join("\n\n") + (value.length > 12 ? "\nMore entries available in the shared board." : "")
    );
  }
  return Object.entries(value)
    .filter(
      ([key]) =>
        !/(^id$|_id$|_cursor$|signature|session_id|token|secret|password|schema)/i.test(key),
    )
    .slice(0, 16)
    .map(([key, entry]) => {
      const content = readableProtocolData(entry, depth + 1);
      const words = key.replaceAll("_", " ").replace(/([a-z])([A-Z])/g, "$1 $2");
      const label = words.charAt(0).toUpperCase() + words.slice(1);
      return content ? `${label}: ${content}` : "";
    })
    .filter(Boolean)
    .join("\n");
}

export function readableProtocolOutput(text: string): string {
  if (!text.trim()) return text;
  text = text
    .replace(/\[notice\] Full-history hydration[^\n]*/g, "")
    .replace(/\[notice\] context compacted/g, "Conversation context summarized.")
    .trim();
  if (!text) return "Conversation restored.";
  const trimmed = text.trim();
  if (!trimmed.startsWith("{") && !/^\[\s*[{"]/.test(trimmed)) return text;
  try {
    return readableProtocolData(JSON.parse(trimmed));
  } catch {
    const lines = trimmed.split("\n").filter(Boolean);
    try {
      return readableProtocolData(lines.map((line) => JSON.parse(line)));
    } catch {
      return /^\s*(?:\{\s*"|\[\s*[{"])/.test(text)
        ? "Structured information received. See the shared board for the current state."
        : text;
    }
  }
}

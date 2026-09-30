export type TimerStopMode = "count" | "date";

export interface TimerStopInput {
  max_deliveries: number | null;
  ends_at: number | null;
}

/** Convert a date-time in the user's local timezone to a valid future stop value. */
export function timerStopInput(
  mode: TimerStopMode,
  maxDeliveries: string,
  localDateTime: string,
  now = Date.now(),
): TimerStopInput | null {
  if (mode === "count") {
    const count = Number(maxDeliveries);
    if (!Number.isInteger(count) || count < 1 || count > 200) return null;
    return { max_deliveries: count, ends_at: null };
  }

  const end = parseLocalDateTime(localDateTime);
  if (end === null || end <= now) return null;
  return { max_deliveries: null, ends_at: end };
}

export function toLocalDateTimeInput(epochMs: number): string {
  const date = new Date(epochMs);
  if (!Number.isFinite(date.getTime())) return "";
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

/** Parse datetime-local's YYYY-MM-DDTHH:mm value as local wall time, rejecting DST gaps. */
export function parseLocalDateTime(value: string): number | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})$/.exec(value);
  if (!match) return null;
  const [, year, month, day, hour, minute] = match.map(Number);
  const date = new Date(year, month - 1, day, hour, minute, 0, 0);
  if (
    date.getFullYear() !== year ||
    date.getMonth() !== month - 1 ||
    date.getDate() !== day ||
    date.getHours() !== hour ||
    date.getMinutes() !== minute
  ) {
    return null;
  }
  return date.getTime();
}

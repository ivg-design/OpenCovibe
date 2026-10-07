import { describe, expect, it } from "vitest";
import { parseLocalDateTime, timerStopInput, toLocalDateTimeInput } from "./timer-stop";

describe("room timer stop values", () => {
  it("round-trips an epoch through a local datetime-local value", () => {
    const epoch = new Date(2026, 6, 15, 9, 7, 0, 0).getTime();
    const local = toLocalDateTimeInput(epoch);
    expect(parseLocalDateTime(local)).toBe(epoch);
  });

  it("accepts count boundaries and serializes only the count mode", () => {
    expect(timerStopInput("count", "1", "", 1000)).toEqual({
      max_deliveries: 1,
      ends_at: null,
    });
    expect(timerStopInput("count", "200", "", 1000)).toEqual({
      max_deliveries: 200,
      ends_at: null,
    });
    expect(timerStopInput("count", "201", "", 1000)).toBeNull();
    expect(timerStopInput("count", "1.5", "", 1000)).toBeNull();
  });

  it("requires a valid future local time and serializes only the date mode", () => {
    const future = new Date(2027, 2, 8, 10, 30).getTime();
    const local = toLocalDateTimeInput(future);
    expect(timerStopInput("date", "10", local, future - 1)).toEqual({
      max_deliveries: null,
      ends_at: future,
    });
    expect(timerStopInput("date", "10", local, future)).toBeNull();
    expect(timerStopInput("date", "10", "not-a-date", 0)).toBeNull();
  });
});

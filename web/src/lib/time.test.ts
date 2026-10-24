import {
  browserZone,
  clock,
  clockOffset,
  formatInZone,
  fromLocalInput,
  relativeDays,
  shortUntil,
  toLocalInput,
} from "./time";

describe("time zones", () => {
  it("formats in the user's zone, not the browser's", () => {
    // 23:30 UTC on 24 Oct is already 25 Oct in Amsterdam (UTC+2 until 25 Oct 01:00 UTC).
    expect(formatInZone("2026-10-24T23:30:00Z", "Europe/Amsterdam", "EEE d MMM HH:mm")).toBe("Sun 25 Oct 01:30");
    expect(formatInZone("2026-10-24T23:30:00Z", "America/New_York", "d MMM HH:mm")).toBe("24 Oct 19:30");
  });
  it("counts relative days in calendar days of that zone", () => {
    const now = new Date("2026-10-24T20:00:00Z");
    expect(relativeDays("2026-10-24T21:00:00Z", "Europe/Amsterdam", now)).toBe("today");
    expect(relativeDays("2026-10-24T23:30:00Z", "Europe/Amsterdam", now)).toBe("tomorrow");
    expect(relativeDays("2026-10-30T08:00:00Z", "Europe/Amsterdam", now)).toBe("in 6 days");
    expect(relativeDays("2026-10-23T08:00:00Z", "Europe/Amsterdam", now)).toBe("yesterday");
    expect(relativeDays("2026-10-20T08:00:00Z", "Europe/Amsterdam", now)).toBe("4 days ago");
  });
  it("has a browser zone", () => {
    expect(browserZone()).toMatch(/\//);
  });
});

describe("countdowns", () => {
  const now = new Date("2026-10-24T12:00:00Z");
  it("shortens to hours or days", () => {
    expect(shortUntil("2026-11-03T08:00:00Z", now)).toBe("10 d");
    expect(shortUntil("2026-10-24T16:30:00Z", now)).toBe("5 h");
    expect(shortUntil("2026-10-24T11:00:00Z", now)).toBe("now");
  });
  it("prints a mock clock", () => {
    expect(clock(42 * 60_000 + 10_000)).toBe("42:10");
    expect(clock(3_725_000)).toBe("1:02:05");
    expect(clock(-5)).toBe("0:00");
  });
  it("measures the server clock offset from the Date header", () => {
    const local = Date.parse("2026-10-24T12:00:00Z");
    expect(clockOffset("Sat, 24 Oct 2026 12:00:05 GMT", local)).toBe(5000);
    expect(clockOffset(null, local)).toBe(0);
    expect(clockOffset("garbage", local)).toBe(0);
  });
});

describe("datetime-local", () => {
  it("round-trips through the user's zone", () => {
    expect(toLocalInput("2026-10-24T07:30:00Z", "Europe/Amsterdam")).toBe("2026-10-24T09:30");
    expect(fromLocalInput("2026-10-24T09:30", "Europe/Amsterdam")?.toISOString()).toBe("2026-10-24T07:30:00.000Z");
    expect(fromLocalInput("2026-11-02T09:30", "Europe/Amsterdam")?.toISOString()).toBe("2026-11-02T08:30:00.000Z");
    expect(fromLocalInput("", "Europe/Amsterdam")).toBeNull();
  });
});

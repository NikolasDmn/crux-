import { describe, expect, it } from "vitest";
import { ApiError } from "@/lib/api";
import { quotaDetail, usageRows, usedShare } from "./storage";

const u = (courseId: string | null, bytes: number) => ({
  courseId,
  bytes,
  originals: 1,
  purgeable: 0,
  pendingPurge: 0,
});

describe("storage", () => {
  it("words a quota error with used and quota", () => {
    const e = new ApiError(
      413,
      "quota_exceeded",
      "x",
      { usedBytes: 98 * 1024 * 1024, quotaBytes: 100 * 1024 * 1024 },
      null,
    );
    expect(quotaDetail(e)).toBe("98 MB of 100 MB used");
    expect(quotaDetail(new ApiError(404, "not_found", "x", null, null))).toBeNull();
    expect(quotaDetail(new Error("x"))).toBeNull();
  });

  it("names courses, biggest first, unknown ones last", () => {
    const rows = usageRows(
      [u(null, 900), u("a", 100), u("gone", 5000), u("b", 300)],
      [
        { id: "a", name: "Statistics", code: "BSA", myRole: "Owner" },
        { id: "b", name: "Algebra", code: "ALG", myRole: "Viewer" },
      ],
    );
    expect(rows.map((r) => [r.name, r.owned])).toEqual([
      ["Algebra", false],
      ["Statistics", true],
      ["A course you're no longer in", false],
      ["Not in a course", false],
    ]);
  });

  it("caps the used share at 1", () => {
    expect(usedShare(50, 100)).toBe(0.5);
    expect(usedShare(150, 100)).toBe(1);
    expect(usedShare(1, 0)).toBe(0);
  });
});

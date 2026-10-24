import { COURSE_COLORS, contrast, courseColor, parseHex, readableOn } from "./color";

it("parses hex colours", () => {
  expect(parseHex("#f76b15")).toEqual([247, 107, 21]);
  expect(parseHex("abc")).toEqual([170, 187, 204]);
  expect(parseHex("orange")).toBeNull();
});

it("computes WCAG contrast", () => {
  expect(contrast("#000000", "#ffffff")).toBeCloseTo(21, 0);
  expect(contrast("#1c1c1a", "#1c1c1a")).toBeCloseTo(1, 5);
});

it("picks a foreground that reads on every course colour (≥ 4.5:1)", () => {
  for (const c of COURSE_COLORS) {
    const fg = readableOn(c.hex);
    expect(contrast(fg, c.hex), `${c.name} with ${fg}`).toBeGreaterThanOrEqual(4.5);
  }
  expect(readableOn("#f76b15")).toBe("#1c1c1a");
  expect(readableOn("#3e63dd")).toBe("#ffffff");
});

it("resolves stored course colours", () => {
  expect(courseColor("#3E63DD").name).toBe("Blue");
  expect(courseColor(null).name).toBe("Orange");
  expect(courseColor("#123456").hex).toBe("#123456");
  expect(courseColor("nope").hex).toBe("#f76b15");
});

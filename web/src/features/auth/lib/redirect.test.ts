import { safeRedirect } from "./redirect";

it("keeps local paths and refuses everything else", () => {
  expect(safeRedirect("/c/01ABC?tab=bank")).toBe("/c/01ABC?tab=bank");
  expect(safeRedirect("https://evil.example/")).toBe("/");
  expect(safeRedirect("//evil.example")).toBe("/");
  expect(safeRedirect("/\\evil.example")).toBe("/");
  expect(safeRedirect("/login?redirect=/x")).toBe("/");
  expect(safeRedirect(undefined, "/inbox")).toBe("/inbox");
});

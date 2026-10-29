import { describe, expect, it } from "vitest";
import { inboxHref } from "./link";

describe("inboxHref", () => {
  it("keeps paths the app has", () => {
    expect(inboxHref("/flash")).toBe("/flash");
    expect(inboxHref("/c/01H/bank?review=import")).toBe("/c/01H/bank?review=import");
    expect(inboxHref("/settings/account")).toBe("/settings/account");
  });

  it("maps notify's spellings to the app's routes", () => {
    expect(inboxHref("/mocks/01M/review")).toBe("/mock/01M/result");
    expect(inboxHref("/c/01H/exams?checkin=01D")).toBe("/c/01H/exams");
  });

  it("has no link for items answered inline, or none at all", () => {
    expect(inboxHref("/courses?invitations=1")).toBeNull();
    expect(inboxHref("/inbox")).toBeNull();
    expect(inboxHref(null)).toBeNull();
    expect(inboxHref("")).toBeNull();
  });
});

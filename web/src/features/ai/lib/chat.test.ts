import { describe, expect, it } from "vitest";
import { applyEvent, citedPage, emptyReply, linkCitations, sameTarget, titleFrom, type AskTarget } from "./chat";

describe("chat", () => {
  it("folds stream events into a reply", () => {
    let r = emptyReply();
    r = applyEvent(r, "delta", { text: "Variance is " });
    r = applyEvent(r, "delta", { text: "spread [p.3]." });
    r = applyEvent(r, "citation", { fileId: "f1", page: 3, chunkNo: 0, quote: "spread" });
    r = applyEvent(r, "done", {
      messageId: "01MSG0000000000000000000007",
      servedBy: null,
      contextUsed: { shared: ["mistakes"] },
      costMicroUsd: 12,
    });
    expect(r).toMatchObject({
      text: "Variance is spread [p.3].",
      messageId: "01MSG0000000000000000000007",
      done: true,
      costMicroUsd: 12,
      contextUsed: { shared: ["mistakes"] },
    });
    expect(r.citations).toHaveLength(1);
    expect(applyEvent(emptyReply(), "error", { code: "provider_auth", message: "bad key" }).error).toEqual({
      code: "provider_auth",
      message: "bad key",
    });
  });

  it("turns page citations into links", () => {
    expect(linkCitations("See [p.12] and [p. 3–4].")).toBe("See [p.12](#p-12) and [p. 3–4](#p-3).");
    expect(linkCitations("[x] stays")).toBe("[x] stays");
    expect(citedPage("#p-12")).toBe(12);
    expect(citedPage("https://x")).toBeNull();
  });

  it("matches conversations to what the panel is on", () => {
    const t: AskTarget = { courseId: "c", scope: "Material", materialId: "m1", title: "x" };
    const c = { scope: "Material", materialId: "m1", componentId: null, questionId: null, mockId: null };
    expect(sameTarget(c, t)).toBe(true);
    expect(sameTarget({ ...c, materialId: "m2" }, t)).toBe(false);
    expect(sameTarget({ ...c, scope: "Course", materialId: null }, { courseId: "c", scope: "Course", title: "" })).toBe(
      true,
    );
  });

  it("titles a conversation from its first line", () => {
    expect(titleFrom("  Why is variance squared?\nmore")).toBe("Why is variance squared?");
    expect(titleFrom("x".repeat(100))).toHaveLength(80);
  });
});

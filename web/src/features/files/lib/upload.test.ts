import { failureText, formatBytes, phaseLabel, precheck, sameOrigin, sha256 } from "./upload";

it("accepts PDFs under the size limit only", () => {
  expect(precheck({ name: "w1.pdf", size: 10, type: "application/pdf" })).toBeNull();
  expect(precheck({ name: "W1.PDF", size: 10, type: "" })).toBeNull();
  expect(precheck({ name: "notes.docx", size: 10, type: "application/msword" })).toMatch(/Only PDFs/);
  expect(precheck({ name: "x.pdf", size: 0, type: "application/pdf" })).toMatch(/empty/);
  expect(precheck({ name: "x.pdf", size: 101 * 1024 * 1024, type: "application/pdf" })).toMatch(/under 100 MB/);
});

it("keeps signed URLs same-origin", () => {
  expect(sameOrigin("http://localhost:8100/crux-files/a/b?X-Amz-Signature=1", "http://localhost:5173")).toBe(
    "/crux-files/a/b?X-Amz-Signature=1",
  );
  expect(sameOrigin("/crux-files/x", "http://localhost:8100")).toBe("/crux-files/x");
});

it("hashes on this thread without a worker", async () => {
  const hex = await sha256(new Blob(["abc"]));
  expect(hex).toBe("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
});

it("labels phases and failures", () => {
  expect(phaseLabel({ phase: "uploading", loaded: 50, total: 200 })).toBe("Uploading 25%");
  expect(phaseLabel({ phase: "uploading", loaded: 0, total: 0 })).toBe("Uploading 0%");
  expect(phaseLabel({ phase: "ready", pageCount: 12 })).toBe("Ready · 12 pages");
  expect(phaseLabel({ phase: "ready", pageCount: null })).toBe("Ready");
  expect(phaseLabel({ phase: "failed", message: "x" })).toBe("x");
  for (const p of ["hashing", "starting", "completing", "scanning", "extracting"] as const) {
    expect(phaseLabel({ phase: p })).toMatch(/…$/);
  }
  expect(failureText("infected")).toMatch(/virus/);
  expect(failureText("failed")).toMatch(/scan/);
  expect(failureText("NotNeeded")).toMatch(/text/);
});

it("formats sizes", () => {
  expect(formatBytes(512)).toBe("512 B");
  expect(formatBytes(2048)).toBe("2 KB");
  expect(formatBytes(1.5 * 1024 * 1024)).toBe("1.5 MB");
  expect(formatBytes(18 * 1024 * 1024)).toBe("18 MB");
  expect(formatBytes(2 * 1024 ** 3)).toBe("2.0 GB");
});

import { expect, test } from "@playwright/test";
import { expectAccessible } from "./support/a11y";
import { apiAs, logIn, RUN, SAMPLE_PDF, seededCourse, signedUp, stackCommand } from "./support/stack";

// Journey 16: free up space on a material, undo, free again; once the original is gone, uploading
// the same PDF restores it. The 7-day wait is skipped with `make purge-now` (CRUX_E2E_PURGE).
test("free up space, undo, free again; re-upload restores", async ({ page }) => {
  const user = await signedUp("saver");
  const api = await apiAs(user.username, user.password);
  const { courseId, lectureId } = await seededCourse(api, `S${RUN.slice(-5).toUpperCase()}`);
  await logIn(page, user);

  await page.goto(`/c/${courseId}/l/${lectureId}`);
  await page.locator('input[type="file"]').setInputFiles(SAMPLE_PDF);
  await expect(page.getByText(/^Ready/)).toBeVisible({ timeout: 45_000 });

  await page.goto(`/c/${courseId}/settings`);
  const row = page.getByRole("listitem").filter({ has: page.getByRole("link", { name: "sample" }) });
  await row.getByRole("button", { name: "Free up space" }).click();
  await expect(row.getByText(/^Goes /)).toBeVisible();
  await expectAccessible(page, "course settings");
  await row.getByRole("button", { name: "Undo" }).click();
  await expect(row.getByRole("button", { name: "Free up space" })).toBeVisible();
  await row.getByRole("button", { name: "Free up space" }).click();
  await expect(row.getByRole("button", { name: "Undo" })).toBeVisible();

  const res = await api.get(`components/${lectureId}/materials`);
  const body = (await res.json()) as
    { title: string; fileId: string | null }[] | { items: { title: string; fileId: string | null }[] };
  const file = (Array.isArray(body) ? body : body.items).find((m) => m.title === "sample")!.fileId!;
  // "Free up space" reaches the files service through an event, and the purge reaches core the
  // same way: each a moment later.
  await expect
    .poll(() => stackCommand("PURGE", "make purge-now FILE={file}", { file }), { timeout: 30_000 })
    .toMatch(/purged [1-9]/);
  await expect
    .poll(
      async () => {
        const list = (await (await api.get(`components/${lectureId}/materials`)).json()) as
          { title: string; originalStatus?: string }[] | { items: { title: string; originalStatus?: string }[] };
        return (Array.isArray(list) ? list : list.items).find((m) => m.title === "sample")?.originalStatus;
      },
      { timeout: 30_000 },
    )
    .toBe("Purged");
  await page.reload();
  await expect(row.getByText("Original removed")).toBeVisible();

  await page.goto(`/c/${courseId}/l/${lectureId}`);
  await page.locator('input[type="file"]').setInputFiles(SAMPLE_PDF);
  await expect(page.getByText("Restored: your questions and notes are reconnected", { exact: true })).toBeVisible({
    timeout: 45_000,
  });
  await page.goto(`/c/${courseId}/settings`);
  await expect(row.getByRole("button", { name: "Free up space" })).toBeVisible();
  await api.dispose();
});

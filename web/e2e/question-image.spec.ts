import { expect, test } from "@playwright/test";
import { expectAccessible } from "./support/axe";
import { apiAs, logIn, RUN, SAMPLE_PDF, seededCourse, signedUp } from "./support/stack";

// The question editor's image: upload one, or crop it from a page of the course's PDFs. Either way
// the saved question shows it.
const PNG = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAABAAAAAQCAIAAACQkWg2AAAAFklEQVR4nGP8z8DAwMDAxMDAwMDAAAANHQEDasKb6QAAAABJRU5ErkJggg==",
  "base64",
);

test("question images: upload, crop from a PDF page, save", async ({ page }) => {
  const user = await signedUp("image");
  const api = await apiAs(user.username, user.password);
  const { courseId, lectureId } = await seededCourse(api, `I${RUN.slice(-5).toUpperCase()}`);
  await api.dispose();
  await logIn(page, user);
  await page.goto(`/c/${courseId}/l/${lectureId}`);
  await page.locator('input[type="file"]').setInputFiles(SAMPLE_PDF);
  await expect(page.getByText(/^Ready/)).toBeVisible({ timeout: 45_000 });

  await page.goto(`/c/${courseId}/bank/new`);
  await page.getByRole("radio", { name: "Open" }).click();
  await page.getByLabel("Lecture", { exact: true }).selectOption({ label: "L1 Basics" });
  await page.getByLabel("Question (markdown, $maths$)").fill(`What does this figure show? (${RUN})`);
  const figure = page.getByRole("img", { name: "Figure for this question" });

  await page
    .locator('input[type="file"][accept*="image"]')
    .setInputFiles({ name: "dot.png", mimeType: "image/png", buffer: PNG });
  await expect(figure).toBeVisible();
  await expect(page.getByRole("button", { name: "Replace image" })).toBeVisible();

  await page.getByRole("button", { name: "Crop from a PDF page" }).click();
  const dialog = page.getByRole("dialog", { name: "Crop from a PDF page" });
  await expect(dialog.getByText(/^of \d+$/)).toBeVisible({ timeout: 20_000 });
  await expectAccessible(page, "crop dialog");
  const area = dialog.locator("canvas");
  const b = (await area.boundingBox())!;
  await page.mouse.move(b.x + 20, b.y + 20);
  await page.mouse.down();
  await page.mouse.move(b.x + 220, b.y + 140, { steps: 5 });
  await page.mouse.up();
  await dialog.getByRole("button", { name: "Use this area" }).click();
  await expect(dialog).toHaveCount(0);
  await expect(figure).toBeVisible();

  await page.getByRole("button", { name: "Create question" }).click();
  await expect(page).toHaveURL(/\/q\/[0-9A-Z]{26}$/);
  await expect(figure).toBeVisible();
  await expect.poll(() => figure.evaluate((img: HTMLImageElement) => img.naturalWidth)).toBeGreaterThan(100);
});

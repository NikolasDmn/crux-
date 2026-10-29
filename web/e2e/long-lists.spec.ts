import { expect, test } from "@playwright/test";
import { apiAs, logIn, RUN, seededCourse, signedUp } from "./support/stack";

// Spec 6: lists over 100 rows are virtualised. 150 questions: only some rows are in the DOM, and
// scrolling still reaches the last one.
test("the question bank virtualises a long list", async ({ page }) => {
  test.setTimeout(120_000);
  const user = await signedUp("lists");
  const api = await apiAs(user.username, user.password);
  const { courseId, lectureId } = await seededCourse(api, `V${RUN.slice(-5).toUpperCase()}`);
  for (let i = 1; i < 150; i++) {
    const r = await api.post(`courses/${courseId}/questions`, {
      data: {
        componentId: lectureId,
        type: "MC",
        content: {
          prompt: `Long list question ${String(i).padStart(3, "0")}`,
          keyReliability: "Official",
          options: [
            { text: "a", isCorrect: true },
            { text: "b", isCorrect: false },
          ],
        },
      },
    });
    expect(r.ok()).toBeTruthy();
  }
  await api.dispose();
  await logIn(page, user);
  await page.goto(`/c/${courseId}/bank`);
  const more = page.getByRole("button", { name: "Load more", exact: true });
  await expect(more).toBeVisible();
  for (let pages = 0; pages < 5 && (await more.count()); pages++) {
    await more.click();
    await expect(more)
      .toBeEnabled({ timeout: 10_000 })
      .catch(() => undefined); // gone once all are here
  }
  await expect(more).toHaveCount(0);
  const rows = page.locator("tbody tr[data-index]");
  const indices = async () => rows.evaluateAll((els) => els.map((e) => Number(e.getAttribute("data-index"))));
  await expect.poll(async () => (await indices()).length).toBeGreaterThan(10);
  expect((await indices()).length, "not every row is rendered").toBeLessThan(120);
  // Scrolling to the end renders the last rows.
  await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
  await expect.poll(async () => Math.max(...(await indices()))).toBe(149);
});

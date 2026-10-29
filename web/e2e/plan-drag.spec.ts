import { expect, test } from "@playwright/test";
import { apiAs, logIn, RUN, seededCourse, signedUp } from "./support/stack";

// The plan calendar: dragging down an empty slot opens "Add to your plan" with that span.
test("drag on the plan calendar to add an entry", async ({ page }) => {
  const user = await signedUp("drag");
  const api = await apiAs(user.username, user.password);
  await seededCourse(api, `D${RUN.slice(-5).toUpperCase()}`);
  await api.dispose();
  await logIn(page, user);
  await page.goto("/plan");
  const day = page.locator('[title="Drag to add, or double-click"]').nth(3);
  const b = (await day.boundingBox())!;
  // From one hour below the top to two hours below it: a one-hour span (48 px an hour).
  await page.mouse.move(b.x + b.width / 2, b.y + 48);
  await page.mouse.down();
  await page.mouse.move(b.x + b.width / 2, b.y + 96, { steps: 6 });
  await page.mouse.up();
  const dialog = page.getByRole("dialog", { name: "Add to your plan" });
  await expect(dialog).toBeVisible();
  const start = await dialog.getByLabel("Start", { exact: true }).inputValue();
  const end = await dialog.getByLabel("End", { exact: true }).inputValue();
  const minutes = (v: string) => Number(v.slice(-5, -3)) * 60 + Number(v.slice(-2));
  expect(minutes(end) - minutes(start)).toBe(60);
});

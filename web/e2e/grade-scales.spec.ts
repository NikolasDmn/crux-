import { expect, test } from "@playwright/test";
import { logIn, signedUp } from "./support/stack";

// Settings > Study > Grade scales: the built-ins a picker offers.
test("choose which grade scales the pickers offer", async ({ page }) => {
  const user = await signedUp("scales");
  await logIn(page, user);
  await page.goto("/courses/new");
  const picker = page.getByLabel("Grade scale", { exact: true });
  await expect(picker.locator("option", { hasText: "German 1.0-5.0" })).toHaveCount(1);

  await page.goto("/settings/study");
  await page.getByLabel("German 1.0-5.0").uncheck();
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect(page.getByText("Saved").first()).toBeVisible();

  await page.goto("/courses/new");
  await expect(picker.locator("option", { hasText: "Percent" })).toHaveCount(1);
  await expect(picker.locator("option", { hasText: "German 1.0-5.0" })).toHaveCount(0);
});

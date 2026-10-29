import { expect, test } from "@playwright/test";
import { expectAccessible } from "./support/axe";
import { logIn, signedUp } from "./support/stack";

// Journey 22: export my data and download the zip; schedule deletion, see the banner, cancel it.
test("export my data; schedule deletion and cancel it", async ({ page }) => {
  test.setTimeout(120_000);
  const user = await signedUp("account");
  await logIn(page, user);

  await page.goto("/settings/account");
  await expectAccessible(page, "account settings");
  await page.getByRole("button", { name: "Export my data" }).click();
  const download = page.getByRole("button", { name: "Download" });
  await expect(download).toBeVisible({ timeout: 90_000 });
  const [file] = await Promise.all([page.waitForEvent("download"), download.click()]);
  expect(file.suggestedFilename()).toMatch(/\.zip$/);

  await page.getByRole("button", { name: "Delete my account…" }).click();
  await page.getByLabel("Your password").fill(user.password);
  await expectAccessible(page, "delete account dialog");
  await page.getByRole("button", { name: "Delete in 14 days" }).click();
  const banner = page.getByText(/^Your account will be deleted on \w{3} /);
  await expect(banner).toBeVisible();
  await page.getByRole("button", { name: "Cancel deletion" }).first().click();
  await expect(banner).toHaveCount(0);
});

import { expect, test } from "@playwright/test";
import { expectAccessible } from "./support/a11y";
import { createInvite, newUser, verifyToken } from "./support/stack";

// Journey 1: sign up with an invite, then verify the email from the link.
test("sign up with an invite", async ({ page }) => {
  const u = newUser("signup");
  const code = await createInvite();

  await page.goto(`/signup?invite=${code}`);
  await expect(page.getByLabel("Invite code")).toHaveValue(code);
  await expectAccessible(page, "signup");
  await page.getByLabel("First name").fill(u.firstName);
  await page.getByLabel("Last name").fill("Tester");
  await page.getByLabel("Email").fill(u.email);
  await page.getByLabel("Username").fill(u.username);
  await page.getByLabel("Password").fill(u.password);
  await page.getByRole("button", { name: "Create account" }).click();
  await expect(page).toHaveURL(/\/$/);
  await expect(page.getByText(/verify your email/i).first()).toBeVisible();

  await page.goto(`/verify?token=${await verifyToken(u.email)}`);
  await expect(page.getByText("Your email is verified.")).toBeVisible();
  await page.goto("/");
  await expect(page.getByText(/verify your email/i)).toHaveCount(0);
});

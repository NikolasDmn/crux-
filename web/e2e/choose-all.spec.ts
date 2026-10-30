import { expect, test } from "@playwright/test";
import { expectAccessible } from "./support/axe";
import { apiAs, logIn, RUN, signedUp } from "./support/stack";

// "Choose all that apply": written in the editor, answered in a mock with the number keys (each
// toggles), graded Right only with every correct option and no wrong one.
test("choose all that apply", async ({ page }) => {
  const user = await signedUp("multi");
  const api = await apiAs(user.username, user.password);
  const course = (await (
    await api.post("courses", { data: { name: "Multi", code: `M${RUN.slice(-5).toUpperCase()}` } })
  ).json()) as {
    id: string;
  };
  await api.post(`courses/${course.id}/components`, { data: { code: "L1", name: "Spread" } });
  await api.dispose();
  await logIn(page, user);

  await page.goto(`/c/${course.id}/bank/new`);
  await page.getByLabel("Lecture", { exact: true }).selectOption({ label: "L1 Spread" });
  await page.getByLabel("Question (markdown, $maths$)").fill(`Which are measures of spread? (${RUN})`);
  await page.getByLabel("Choose all that apply").check();
  await page.getByLabel("Option A", { exact: true }).fill("Variance");
  await page.getByLabel("Option B", { exact: true }).fill("Range");
  await page.getByLabel("Option C", { exact: true }).fill("Mean");
  await page.getByLabel("Option D", { exact: true }).fill("Median");
  await page.getByRole("checkbox", { name: "Option A is correct" }).click();
  await page.getByRole("checkbox", { name: "Option B is correct" }).click();
  await expectAccessible(page, "editor, choose all that apply");
  await page.getByRole("button", { name: "Create question" }).click();
  await expect(page).toHaveURL(/\/q\/[0-9A-Z]{26}$/);

  await page.goto(`/c/${course.id}/mocks`);
  await page
    .getByRole("button", { name: /New mock|Build a mock/ })
    .first()
    .click();
  await page.getByRole("button", { name: "Build mock" }).click();
  await page.getByRole("button", { name: "Start" }).click();
  await expect(page.getByText("Choose all that apply.")).toBeVisible();
  const options = page.getByRole("group", { name: "Options, choose all that apply" }).getByRole("checkbox");
  const texts = await options.allTextContents();
  for (const want of ["Variance", "Range"]) {
    await page.keyboard.press(`Digit${texts.findIndex((t) => t.includes(want)) + 1}`);
  }
  await expect(options.filter({ hasText: "Variance" })).toHaveAttribute("aria-checked", "true");
  await expect(options.filter({ hasText: "Range" })).toHaveAttribute("aria-checked", "true");
  await expect(options.filter({ hasText: "Mean" })).toHaveAttribute("aria-checked", "false");
  await expectAccessible(page, "mock, choose all that apply");
  await page.keyboard.press("Space");
  await page.getByRole("button", { name: "Hand in" }).click();
  await expect(page).toHaveURL(/\/result/);
  await expect(
    page.getByRole("region", { name: "Question 1" }).getByText("Right", { exact: true }).first(),
  ).toBeVisible();
});

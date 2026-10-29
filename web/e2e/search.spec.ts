import { expect, test } from "@playwright/test";
import { apiAs, logIn, RUN, seededCourse, signedUp } from "./support/stack";

// The command palette finds a lecture inside a course by its name.
test("search finds a lecture inside a course", async ({ page }) => {
  const user = await signedUp("search");
  const api = await apiAs(user.username, user.password);
  const { courseId } = await seededCourse(api, `S${RUN.slice(-5).toUpperCase()}`);
  const l = await api.post(`courses/${courseId}/components`, {
    data: { code: "L6", name: "Simple linear regression" },
  });
  const lectureId = ((await l.json()) as { id: string }).id;
  await api.dispose();
  await logIn(page, user);
  await page.keyboard.press("Control+k");
  await page.getByRole("combobox").fill("simple linear");
  await page.getByRole("option", { name: /Simple linear regression/ }).click();
  await expect(page).toHaveURL(new RegExp(`/c/${courseId}/l/${lectureId}`));
});

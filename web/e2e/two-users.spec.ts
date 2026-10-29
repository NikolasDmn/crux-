import { expect, test, type Browser, type Page } from "@playwright/test";
import { expectAccessible } from "./support/a11y";
import { apiAs, logIn, RUN, seededCourse, signedUp, type User } from "./support/stack";

// Journeys 17 and 18: another user's mock is a 404 page; sharing a course by username, accepted
// from the sidebar strip.
async function pageFor(browser: Browser, u: User): Promise<Page> {
  const page = await (await browser.newContext()).newPage();
  await logIn(page, u);
  return page;
}

test("17-18. privacy between users, and sharing a course", async ({ browser }) => {
  const [ann, ben] = [await signedUp("ann"), await signedUp("ben")];
  const api = await apiAs(ann.username, ann.password);
  const code = `P${RUN.slice(-5).toUpperCase()}`;
  const { courseId } = await seededCourse(api, code);
  const mock = await api.post(`courses/${courseId}/mocks`, { data: { name: "Ann's mock", n: 5 } });
  expect(mock.ok(), "build Ann's mock").toBeTruthy();
  const mockId = ((await mock.json()) as { id: string }).id;

  // 17: Ben can't open Ann's mock, or its result.
  const b = await pageFor(browser, ben);
  for (const path of [`/mock/${mockId}`, `/mock/${mockId}/result`]) {
    await b.goto(path);
    await expect(b.getByRole("heading", { name: "Not found, or you don't have access." })).toBeVisible();
  }
  await expectAccessible(b, "not found");

  // 18: Ann shares the course with Ben by username; Ben accepts from the sidebar.
  const a = await pageFor(browser, ann);
  await a.goto(`/c/${courseId}/settings`);
  await a.getByLabel("Username or email").fill(ben.username);
  await a.getByRole("button", { name: "Share", exact: true }).click();
  await expect(
    a
      .getByText(ben.username)
      .or(a.getByText(`${ben.firstName} Tester`))
      .first(),
  ).toBeVisible();

  await b.goto("/");
  const strip = b.getByText(new RegExp(`shared ${code} with you`));
  await expect(strip).toBeVisible({ timeout: 20_000 });
  await expectAccessible(b, "home with an invitation");
  await b.getByRole("button", { name: "Accept" }).click();
  await expect(strip).toHaveCount(0);
  await b.getByLabel("Main").getByRole("link", { name: code }).click();
  await expect(b.getByRole("heading", { name: `Course ${code}`, level: 1 })).toBeVisible();
  await expect(b.getByText(/Shared with you by/)).toHaveCount(0); // that line is on Settings
  await api.dispose();
});

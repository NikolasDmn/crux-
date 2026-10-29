import { expect, test, type Page } from "@playwright/test";
import { expectAccessible } from "./support/axe";
import { apiAs, logIn, RUN, SAMPLE_PDF, seededCourse, signedUp, type User } from "./support/stack";

// Journeys 15 and 21, with the dev fake provider (keys starting `fake-valid`): turn AI on and add a
// key; ask about a PDF selection and follow the citation; AI-grade a mock and accept all; set up a
// course with AI; import into an existing course, where a lecture it has shows as a match.
test.describe.configure({ mode: "serial" });

let page: Page;
let user: User;
let courseId = "";
let lectureId = "";
const CODE = `A${RUN.slice(-5).toUpperCase()}`;
const OPEN_PROMPT = `Why do we standardise a test statistic? (${RUN})`;

test.beforeAll(async ({ browser }) => {
  user = await signedUp("ai");
  const api = await apiAs(user.username, user.password);
  ({ courseId, lectureId } = await seededCourse(api, CODE));
  const open = await api.post(`courses/${courseId}/questions`, {
    data: {
      componentId: lectureId,
      type: "Open",
      content: {
        prompt: OPEN_PROMPT,
        keyReliability: "Official",
        open: { modelAnswer: "To compare it with a known distribution." },
      },
    },
  });
  expect(open.ok(), "an open question").toBeTruthy();
  await api.dispose();
  page = await (await browser.newContext()).newPage();
  await logIn(page, user);
});
test.afterAll(async () => {
  await page.context().close();
});

async function uploadReady(p: Page) {
  await p.locator('input[type="file"]').setInputFiles(SAMPLE_PDF);
  await expect(p.getByText(/^Ready/).first()).toBeVisible({ timeout: 45_000 });
}

test("15a. turn AI on and add a key", async () => {
  await page.goto("/settings/ai");
  await expectAccessible(page, "AI settings, off");
  await page.getByRole("button", { name: "I agree, turn on AI" }).click();
  await page.getByLabel("Provider", { exact: true }).selectOption({ label: "Fake (dev only)" });
  await page.getByLabel("Label", { exact: true }).fill("e2e");
  await page.getByLabel("API key", { exact: true }).fill(`fake-valid-${RUN}`);
  await page.getByRole("button", { name: "Add key" }).click();
  await expect(page.getByText(/^Checked /)).toBeVisible();
  await expectAccessible(page, "AI settings, on");
});

test("15b. ask about a PDF selection; the citation opens the page", async () => {
  await page.goto(`/c/${courseId}/l/${lectureId}`);
  await uploadReady(page);
  await page.getByRole("link", { name: "sample" }).first().click();
  const words = page.locator("[data-page] .textLayer span").filter({ hasText: /\w{3}/ }).first();
  await expect(words).toBeAttached({ timeout: 20_000 });
  await words.click({ clickCount: 3 });
  await page.getByRole("button", { name: "Ask AI" }).last().click();
  const panel = page.getByRole("complementary", { name: "Ask AI" }).or(page.getByRole("dialog", { name: /Ask AI/ }));
  await expect(panel.getByText(/About p\.\d+/)).toBeVisible();
  await panel.getByLabel("Your question").fill("What does this mean?");
  await panel.getByLabel("Your question").press("Enter");
  const chip = panel.getByRole("button", { name: "p.1", exact: true }).first();
  await expect(chip).toBeVisible({ timeout: 20_000 });
  await expectAccessible(page, "ask AI panel");
  await chip.click();
  await expect(page).toHaveURL(/[?&]page=1/);
});

test("15c. AI-grade a mock and accept all", async () => {
  await page.goto(`/c/${courseId}/mocks`);
  await page
    .getByRole("button", { name: /New mock|Build a mock/ })
    .first()
    .click();
  await page.getByLabel("Name", { exact: true }).fill("AI graded");
  await page.getByLabel("Grade open answers with AI").check();
  await page.getByRole("button", { name: "Build mock" }).click();
  await page.getByRole("button", { name: "Start" }).click();
  for (let i = 0; i < 2; i++) {
    await expect(page.getByText(`Question ${i + 1} of 2`)).toBeVisible();
    if (await page.getByText(OPEN_PROMPT).isVisible()) {
      await page
        .getByLabel("Your answer", { exact: true })
        .fill("So it can be compared with a reference distribution.");
      await page.getByLabel("Your answer", { exact: true }).blur();
    } else {
      await page.keyboard.press("Digit1");
    }
    await page.keyboard.press("Space");
  }
  await page.getByRole("button", { name: "Hand in" }).click();
  await expect(page).toHaveURL(/\/result/);
  const acceptAll = page.getByRole("button", { name: "Accept all" });
  await expect(acceptAll).toBeVisible({ timeout: 45_000 });
  await expectAccessible(page, "AI grading review");
  await acceptAll.click();
  const open = page.getByRole("region").filter({ hasText: OPEN_PROMPT });
  await expect(open.getByText("Partial", { exact: true })).toBeVisible();
});

test("15d. set up a course with AI and apply", async () => {
  await page.goto("/courses/new");
  await page.getByRole("radio", { name: "Set up with AI" }).click();
  await uploadReady(page);
  await page.getByLabel("Turn past exams into questions").check(); // question proposals too
  await page.getByRole("button", { name: "Read my files" }).click();
  await expect(page).toHaveURL(/\/imports\/[0-9A-Z]{26}/);
  const apply = page.getByRole("button", { name: "Apply accepted" });
  await expect(apply).toBeVisible({ timeout: 60_000 });
  await expect(page.getByText("Demo import").first()).toBeVisible();
  await expectAccessible(page, "import review");
  // A proposed MC question's options can be fixed before applying.
  const question = page.getByRole("listitem").filter({ hasText: "Which statement about" });
  await question.getByRole("button", { name: "Edit" }).click();
  const edit = page.getByRole("dialog", { name: "Edit question" });
  await edit.getByLabel("Option B", { exact: true }).fill("The second one, reworded");
  await edit.getByLabel("Option B is correct").check();
  await edit.getByRole("button", { name: "Add option" }).click();
  await expect(edit.getByText("Fill in every option, or remove the empty one.")).toBeVisible();
  await edit.getByLabel("Option C", { exact: true }).fill("A third one");
  await expectAccessible(page, "edit proposal");
  await edit.getByRole("button", { name: "Save" }).click();
  await expect(edit).toHaveCount(0);
  await expect(question.getByText("Edited")).toBeVisible();
  await expect(question.getByText("The second one, reworded")).toBeVisible();
  await apply.click();
  await expect(page).toHaveURL(/\/c\/[0-9A-Z]{26}/, { timeout: 30_000 });
  await expect(page.getByRole("heading", { name: "Demo import", level: 1 })).toBeVisible();
});

test("21. import into an existing course: the lecture it has is a match", async () => {
  await page.goto(`/c/${courseId}/lectures`);
  await page.getByRole("button", { name: "Import with AI" }).click();
  const dialog = page.getByRole("dialog", { name: "Import with AI" });
  await expectAccessible(page, "import with AI");
  await uploadReady(page);
  await dialog.getByRole("button", { name: "Read my files" }).click();
  await expect(page).toHaveURL(/\/imports\/[0-9A-Z]{26}/);
  const match = page.getByRole("link", { name: "Matches your L1 Basics" });
  await expect(match).toBeVisible({ timeout: 60_000 });
  await page.getByRole("button", { name: "Apply accepted" }).click();
  await expect(page).toHaveURL(new RegExp(`/c/${courseId}`), { timeout: 30_000 });
  await page.goto(`/c/${courseId}/lectures`);
  await expect(page.getByRole("link", { name: /Basics/ })).toHaveCount(1);
  await expect(page.getByRole("link", { name: "Core methods" })).toBeVisible();
});

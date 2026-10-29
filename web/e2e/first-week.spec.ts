import { expect, test, type Page } from "@playwright/test";
import { expectAccessible } from "./support/a11y";
import { logIn, RUN, SAMPLE_PDF, signedUp, type User } from "./support/stack";

// A user's first week (frontend-spec 9, journeys 2-14, 19, 20): one course built by hand, a mock in
// the real exam layout taken with the keyboard, the review, flashcards, Home, This week, Take me
// there, the study window and the Trash. Each step builds on the one before.
test.describe.configure({ mode: "serial" });

let page: Page;
let user: User;
let course = "";
const CODE = `W${RUN.slice(-5).toUpperCase()}`;
const MC_PROMPT = `Which measure is robust to outliers? (${RUN})`;
const NUM_PROMPT = `What is the mean of 2, 4 and 6? (${RUN})`;
const OPEN_PROMPT = `Explain what a p-value is. (${RUN})`;

test.beforeAll(async ({ browser }) => {
  user = await signedUp("week");
  page = await (await browser.newContext()).newPage();
  await logIn(page, user);
});
test.afterAll(async () => {
  await page.context().close();
});

test("2. create a course manually", async () => {
  await page.goto("/courses/new");
  await page.getByLabel("Name", { exact: true }).fill("Week one statistics");
  await page.getByLabel("Code", { exact: true }).fill(CODE);
  await page.getByRole("button", { name: "Create course" }).click();
  await expect(page).toHaveURL(/\/c\/[0-9A-Z]{26}$/);
  course = /\/c\/([0-9A-Z]{26})/.exec(page.url())![1]!;
  await expectAccessible(page, "course overview");

  await page.getByRole("link", { name: "Lectures", exact: true }).click();
  await page.getByLabel("Code", { exact: true }).fill("L1");
  await page.getByLabel("Name", { exact: true }).fill("Descriptive statistics");
  await page.getByRole("button", { name: "Add lecture" }).click();
  await expect(page.getByRole("link", { name: /Descriptive statistics/ })).toBeVisible();
  await expectAccessible(page, "lectures");
});

test("3. upload a PDF", async () => {
  await page.getByRole("link", { name: /Descriptive statistics/ }).click();
  await page.locator('input[type="file"]').setInputFiles(SAMPLE_PDF);
  await expect(page.getByText(/^Ready/)).toBeVisible({ timeout: 45_000 });
  await expect(page.getByRole("link", { name: /sample/i })).toBeVisible();
  await expectAccessible(page, "lecture");
});

async function newQuestion(type: "Multiple choice" | "Numeric" | "Open", prompt: string) {
  await page.goto(`/c/${course}/bank/new`);
  await page.getByRole("radio", { name: type }).click();
  await page.getByLabel("Lecture", { exact: true }).selectOption({ label: "L1 Descriptive statistics" });
  await page.getByLabel("Question (markdown, $maths$)", { exact: true }).fill(prompt);
}

test("4. add MC, numeric and open questions", async () => {
  await newQuestion("Multiple choice", MC_PROMPT);
  await page.getByLabel("Option A", { exact: true }).fill("The median");
  await page.getByLabel("Option B", { exact: true }).fill("The mean");
  await page.getByLabel("Option A is correct", { exact: true }).check();
  await expectAccessible(page, "question editor");
  await page.getByRole("button", { name: "Create question" }).click();
  await expect(page).toHaveURL(/\/q\/[0-9A-Z]{26}$/);

  await newQuestion("Numeric", NUM_PROMPT);
  await page.getByLabel("Value", { exact: true }).fill("4");
  await page.getByRole("button", { name: "Create question" }).click();
  await expect(page).toHaveURL(/\/q\/[0-9A-Z]{26}$/);

  await newQuestion("Open", OPEN_PROMPT);
  await page
    .getByLabel("Model answer", { exact: true })
    .fill("The chance of data this extreme if the null hypothesis holds.");
  await page.getByRole("button", { name: "Add criterion" }).click();
  await page.getByLabel("Criterion 1", { exact: true }).fill("Mentions the null hypothesis");
  await page.getByLabel("Points for criterion 1", { exact: true }).fill("1");
  await page.getByRole("button", { name: "Create question" }).click();
  await expect(page).toHaveURL(/\/q\/[0-9A-Z]{26}$/);
  await expectAccessible(page, "question");

  await page.goto(`/c/${course}/bank`);
  for (const p of [MC_PROMPT, NUM_PROMPT, OPEN_PROMPT]) await expect(page.getByText(p)).toBeVisible();
  await expectAccessible(page, "bank");
});

test("5. build a mock in the real exam layout", async () => {
  await page.goto(`/c/${course}/exams`);
  await page.getByRole("button", { name: "Add the exam" }).click();
  const when = new Date(Date.now() + 21 * 86_400_000);
  const local = `${when.toISOString().slice(0, 10)}T09:00`;
  await page.getByLabel("Starts", { exact: true }).fill(local);
  await page.getByLabel("Title", { exact: true }).fill("Final exam");
  await page.getByLabel("Duration (minutes)", { exact: true }).fill("120");
  await page.getByRole("button", { name: "Add", exact: true }).click();
  await expect(page.getByText("Final exam")).toBeVisible();
  await expectAccessible(page, "exams");

  await page.getByRole("button", { name: /Actions for/ }).click();
  await page.getByRole("menuitem", { name: "Exam layout" }).click();
  await page.getByLabel("Questions from L1", { exact: true }).fill("3");
  await page.getByLabel("Open questions from L1", { exact: true }).fill("1");
  await page.getByRole("button", { name: "Save layout" }).click();
  await expect(page.getByText("Layout: 3 questions")).toBeVisible();

  await page.getByRole("link", { name: "Mocks", exact: true }).click();
  await page
    .getByRole("button", { name: /New mock|Build a mock/ })
    .first()
    .click();
  await expect(page.getByRole("dialog", { name: "New mock" })).toBeVisible();
  await page.getByLabel("Name", { exact: true }).fill("Dress rehearsal");
  await expect(page.getByLabel(/Real exam layout \(3 questions/)).toBeChecked();
  await expectAccessible(page, "new mock");
  await page.getByRole("button", { name: "Build mock" }).click();
  await expect(page).toHaveURL(/\/mock\/[0-9A-Z]{26}/);
});

test("6-7. answer with keyboard shortcuts, then submit", async () => {
  await page.getByRole("button", { name: "Start" }).click();
  await expectAccessible(page, "mock");
  for (let i = 0; i < 3; i++) {
    const prompt = page.getByText(new RegExp(`\\(${RUN}\\)`)).first();
    const text = (await prompt.textContent()) ?? "";
    if (text.includes("robust")) {
      // A confident miss, by digit (options are shuffled): the mean, sure.
      const options = page.getByRole("radiogroup", { name: "Options" }).getByRole("radio");
      const n = (await options.allTextContents()).findIndex((t) => t.includes("The mean"));
      await page.keyboard.press(`Digit${n + 1}`);
      await expect(options.nth(n)).toHaveAttribute("aria-checked", "true");
    } else if (text.includes("mean of")) {
      await page.getByLabel("Your answer", { exact: true }).fill("4");
      await page.getByLabel("Your answer", { exact: true }).blur();
    } else {
      await page.getByLabel("Your answer", { exact: true }).fill("It's the probability the null hypothesis is true.");
      await page.getByLabel("Your answer", { exact: true }).blur();
    }
    await page.keyboard.press("Space");
  }
  await expect(page.getByRole("dialog", { name: "Hand it in?" })).toBeVisible();
  await page.getByRole("button", { name: "Hand in" }).click();
  await expect(page).toHaveURL(/\/mock\/[0-9A-Z]{26}\/result/);
});

const item = (prompt: string) => page.getByRole("region").filter({ hasText: prompt });

test("8. self-grade the open answer", async () => {
  const open = item(OPEN_PROMPT);
  await expect(open.getByText("To mark")).toBeVisible();
  await expectAccessible(page, "mock result");
  await open.getByRole("button", { name: "Partly" }).click();
  await expect(open.getByText("Partial", { exact: true })).toBeVisible();
});

test("9. reveal and fix", async () => {
  const mc = item(MC_PROMPT);
  await expect(mc.getByText("Wrong", { exact: true })).toBeVisible();
  await mc.getByRole("button", { name: "Show answer" }).click();
  await expect(mc.getByRole("button", { name: "Show answer" })).toHaveCount(0);
  await mc.getByRole("button", { name: "I fixed this" }).click();
  await expect(mc.getByText("Fixed")).toBeVisible();
});

test("10. confirm a slip", async () => {
  const mc = item(MC_PROMPT);
  await mc.getByRole("button", { name: "Log a slip" }).click();
  await mc.getByLabel("What went wrong", { exact: true }).fill("Mixed up mean and median");
  await expectAccessible(page, "slip form");
  await mc.getByRole("button", { name: "Log slip" }).click();
  await expect(mc.getByRole("button", { name: "Log a slip" })).toHaveCount(0);
});

test("11. make a flashcard", async () => {
  await item(MC_PROMPT).getByRole("button", { name: "Make a flashcard" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  await expectAccessible(page, "flashcard dialog");
  await dialog
    .getByRole("button", { name: /Save|Add|Create/ })
    .last()
    .click();
  await expect(dialog).toHaveCount(0);
});

test("12. review flashcards", async () => {
  await page.goto("/flash/review");
  await expect(page.getByText(MC_PROMPT)).toBeVisible();
  await expectAccessible(page, "flashcard review");
  await page.keyboard.press("Space");
  await expect(page.getByText("The median").first()).toBeVisible();
  // Flagged for the warm-up that Take me there opens with (journey 14).
  await page.getByLabel("Include in my next warm-up").check();
  await page.keyboard.press("Digit3");
  await expect(page.getByRole("heading", { name: "1 card reviewed" })).toBeVisible();
  await expectAccessible(page, "review summary");
});

test("13. check Home and This week", async () => {
  await page.goto("/");
  await expect(page.getByText(CODE).first()).toBeVisible();
  await expect(page.getByText(/Final exam/).first()).toBeVisible();
  await expectAccessible(page, "home");
  await page.goto("/week");
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  await expectAccessible(page, "this week");
});

test("14. Take me there: warm-up, then the lecture", async () => {
  await page.goto("/next");
  await expect(page.getByRole("heading", { name: "Take me there" })).toBeVisible();
  const steps = page.getByRole("list", { name: "Steps" });
  await expect(steps).toBeVisible();
  await expectAccessible(page, "take me there");
  const first = steps.getByRole("listitem").first();
  await expect(first).toHaveAttribute("aria-current", "step");
  await expect(first).toContainText("Warm-up");
  await expect(page.getByText("Warm-up 1 of 1")).toBeVisible();
  await expect(page.getByText(MC_PROMPT)).toBeVisible();
  await page.getByRole("button", { name: "Show answer" }).click();
  await page.getByRole("button", { name: /^Good/ }).click();

  await expect(steps.getByRole("listitem").nth(1)).toHaveAttribute("aria-current", "step");
  await expect(steps.getByRole("listitem").first().getByLabel("Done")).toBeVisible();
  await page.getByRole("link", { name: "Open the lecture" }).click();
  await expect(page).toHaveURL(/\/l\/[0-9A-Z]{26}/);
  await expect(page.getByRole("heading", { name: /Descriptive statistics/ })).toBeVisible();
});

test("20. study window: set 18:00-21:00 and see the plan move", async () => {
  await page.goto("/settings/study");
  await page.getByLabel("Study from", { exact: true }).selectOption("18:00");
  await page.getByLabel("Until", { exact: true }).selectOption("21:00");
  await expectAccessible(page, "study settings");
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.getByRole("button", { name: "Save" })).toBeDisabled();
  // The lecture moves into the evening; the calendar shows only the window's hours.
  await page.goto("/plan");
  await expect(page.getByRole("heading", { name: "Plan", level: 1 })).toBeVisible();
  const blocks = page.getByText(/^\d{2}:\d{2}–\d{2}:\d{2}$/);
  await expect
    .poll(async () => (await blocks.allTextContents()).some((t) => t >= "18:00" && t.slice(6) <= "21:00"), {
      message: "a session between 18:00 and 21:00",
    })
    .toBe(true);
  await expect(page.getByText("19:00", { exact: true })).toBeVisible();
  await expect(page.getByText("09:00", { exact: true })).toHaveCount(0);
  await expectAccessible(page, "plan");
});

test("19. trash a course and restore it", async () => {
  await page.goto("/courses/new");
  await page.getByLabel("Name", { exact: true }).fill("Throwaway");
  await page.getByLabel("Code", { exact: true }).fill(`T${RUN.slice(-5).toUpperCase()}`);
  await page.getByRole("button", { name: "Create course" }).click();
  await expect(page).toHaveURL(/\/c\/[0-9A-Z]{26}$/);
  await page.getByLabel("Course").getByRole("link", { name: "Settings" }).click();
  await page.getByRole("button", { name: "Delete course" }).click();
  await page
    .getByRole("dialog")
    .getByRole("button", { name: /Delete|Move to Trash/ })
    .click();
  await expect(page).toHaveURL(/\/trash$/);
  await expect(page.getByText("Throwaway")).toBeVisible();
  await expectAccessible(page, "trash");
  await page.getByRole("button", { name: "Restore" }).click();
  // Restored: it opens again, and it's back in the sidebar.
  await expect(page.getByRole("heading", { name: "Throwaway", level: 1 })).toBeVisible();
  await expect(page.getByLabel("Main").getByRole("link", { name: `T${RUN.slice(-5).toUpperCase()}` })).toBeVisible();
});

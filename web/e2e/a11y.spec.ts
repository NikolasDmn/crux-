import { expect, test, type APIRequestContext, type Page } from "@playwright/test";
import { expectAccessible } from "./support/axe";
import { apiAs, logIn, RUN, seededCourse, signedUp } from "./support/stack";

// Spec 9 and 2.7: every screen in section 4 through axe (0 serious or critical), in light and dark,
// at 1280 and 375 px. One user with a small course behind every route; one signed-in page.

const MODES = [
  { width: 1280, height: 800, scheme: "light" },
  { width: 1280, height: 800, scheme: "dark" },
  { width: 375, height: 812, scheme: "light" },
  { width: 375, height: 812, scheme: "dark" },
] as const;

let page: Page;
const ids: Record<string, string> = {};

async function post(api: APIRequestContext, path: string, data?: unknown): Promise<{ id: string }> {
  const res = await api.post(path, data === undefined ? {} : { data });
  expect(res.ok(), `${path} ${res.status()}`).toBeTruthy();
  return (await res.json()) as { id: string };
}

test.beforeAll(async ({ browser }) => {
  const user = await signedUp("a11y");
  const api = await apiAs(user.username, user.password);
  const { courseId, lectureId } = await seededCourse(api, `X${RUN.slice(-5).toUpperCase()}`);
  Object.assign(ids, { courseId, lectureId });
  ids.materialId = (
    await post(api, `components/${lectureId}/materials`, {
      kind: "Notes",
      title: "Notes",
      content: "# Mean\n\nThe mean is $\\bar{x} = \\frac{1}{n}\\sum x_i$.",
    })
  ).id;
  const questions = (await (await api.get(`courses/${courseId}/questions`)).json()) as
    { id: string }[] | { items: { id: string }[] };
  ids.questionId = (Array.isArray(questions) ? questions : questions.items)[0]!.id;
  await post(api, `courses/${courseId}/flashcards`, { componentId: lectureId, front: "Mean?", back: "Sum over n" });
  await post(api, `courses/${courseId}/papers`, { paperKey: "2025-06", label: "June 2025", examDate: "2025-06-20" });
  ids.openMock = (await post(api, `courses/${courseId}/mocks`, { name: "Not started", n: 5 })).id;
  ids.doneMock = (await post(api, `courses/${courseId}/mocks`, { name: "Handed in", n: 5 })).id;
  await post(api, `mocks/${ids.doneMock}/start`);
  await post(api, `mocks/${ids.doneMock}/submit`);
  await api.dispose();
  page = await (await browser.newContext()).newPage();
  await logIn(page, user);
});
test.afterAll(async () => {
  await page.context().close();
});

const c = () => `/c/${ids.courseId}`;
const SCREENS: [string, () => string][] = [
  ["home", () => "/"],
  ["take me there", () => "/next"],
  ["this week", () => "/week"],
  ["plan", () => "/plan"],
  ["mistakes", () => "/insight"],
  ["flashcards", () => "/flash"],
  ["flashcard review", () => "/flash/review"],
  ["inbox", () => "/inbox"],
  ["trash", () => "/trash"],
  ["new course", () => "/courses/new"],
  ["settings: profile", () => "/settings/profile"],
  ["settings: study", () => "/settings/study"],
  ["settings: notifications", () => "/settings/notifications"],
  ["settings: storage", () => "/settings/storage"],
  ["settings: AI", () => "/settings/ai"],
  ["settings: account", () => "/settings/account"],
  ["course overview", () => c()],
  ["lectures", () => `${c()}/lectures`],
  ["lecture", () => `${c()}/l/${ids.lectureId}`],
  ["material", () => `${c()}/m/${ids.materialId}`],
  ["questions", () => `${c()}/bank`],
  ["new question", () => `${c()}/bank/new`],
  ["question", () => `${c()}/q/${ids.questionId}`],
  ["edit question", () => `${c()}/q/${ids.questionId}/edit`],
  ["past papers", () => `${c()}/papers`],
  ["mocks", () => `${c()}/mocks`],
  ["cards", () => `${c()}/cards`],
  ["course plan", () => `${c()}/plan`],
  ["exams", () => `${c()}/exams`],
  ["course settings", () => `${c()}/settings`],
  ["mock start", () => `/mock/${ids.openMock}`],
  ["mock result", () => `/mock/${ids.doneMock}/result`],
  ["not found", () => "/no-such-page"],
];

for (const [name, path] of SCREENS) {
  test(name, async () => {
    for (const m of MODES) {
      await page.setViewportSize({ width: m.width, height: m.height });
      await page.emulateMedia({ colorScheme: m.scheme, reducedMotion: "reduce" });
      await page.goto(path());
      await expect(page.locator("main, [role=main], body > div").first()).toBeVisible();
      // Content, not skeletons: axe on a loading screen proves little.
      await expect(page.locator(".animate-pulse[aria-hidden=true]")).toHaveCount(0, { timeout: 15_000 });
      await expectAccessible(page, `${name} (${m.width}px, ${m.scheme})`, true);
      const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
      expect.soft(overflow, `${name} scrolls sideways at ${m.width}px`).toBeLessThanOrEqual(0);
    }
  });
}

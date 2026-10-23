# Crux: frontend spec

> Version 1.0, 2026-10-23. Audience: an engineering agent that builds the web app from this document plus `backend-spec.md` (the API it calls). **Section 3 (Design) is intentionally empty**: Nick fills it in a separate frontend design thread. Until then, build with unstyled shadcn/ui defaults and the token names in 3.1, so restyling later changes only tokens and component skins, never screen code.

## 0. Decisions (Nick, 2026-10-23)

| Topic | Decision |
|---|---|
| Framework | React 18 + TypeScript (strict) |
| Build / app type | Vite single-page app; static files served by the gateway (no Node server in production) |
| Styling | Tailwind CSS + shadcn/ui (Radix primitives), restyled later by the design thread |
| Data | TanStack Query. The API client and types are generated from the gateway's merged OpenAPI (`openapi-typescript` + `openapi-fetch`); never hand-write API types |
| Routing | TanStack Router (file-based, type-safe params and search params) |
| Maths | KaTeX for inline `$…$` and display `$$…$$`; MathJax 3 lazy-loaded as a fallback for any expression KaTeX rejects |
| PDF | pdf.js (`pdfjs-dist`) viewer with text selection → "Ask AI about this" |
| Theme | Light and dark with a toggle (system / light / dark) |
| Layout | Desktop-first with a left sidebar. More than usable on a phone: mocks, flashcards and "Take me there" are fully usable at 360 px wide |
| Feel | Calm and minimal, like Notion or Linear (details in section 3, empty for now) |
| Language | English only. All strings go through one `t()` helper with English keys, so translation can be added later without touching components |
| Accessibility | WCAG 2.2 AA |
| Browsers | Latest 2 versions of Chrome, Firefox, Safari and Edge; iOS Safari and Android Chrome |
| Tests | Vitest + Testing Library (80% line coverage on `lib/` logic and hooks); Playwright for the main flows (section 9) |
| Offline / PWA | Later. Keep `clientId` on reviews, attempts and events so offline sync can be added without API changes |
| Name / logo | Crux; mark A from `brand/crux-mark-{light,dark}.svg`. The dot takes the current course's colour (section 5.2) |

## 1. Project layout

```
web/
  src/
    app/            router setup, providers (QueryClient, Theme, Auth, Toaster), error boundary
    routes/         TanStack Router file routes (section 4)
    features/       one folder per feature: courses, lectures, materials, bank, mocks, flash,
                    insight, plan, next, ai, files, notify, settings, admin, auth
      <feature>/
        api.ts      query/mutation hooks wrapping the generated client (the only place that calls the API)
        components/ feature components
        lib/        pure logic (unit-tested): formatting, scoring display, key-hiding rules
    components/     shared UI built on shadcn/ui (Button, Card, Dialog, DataTable, Math, Markdown, ...)
    lib/            api client, sse, auth, ids (ULID), time (user time zone), keyboard, t()
    styles/         tokens.css (section 3.1), tailwind.css
  tests/e2e/        Playwright
```

Rules:

- Components never call `fetch`. They use the hooks in `features/*/api.ts`.
- Pure logic lives in `lib/` and has unit tests.
- No global state library. Server state lives in TanStack Query. Small client state (theme, sidebar open, current mock answers before save) lives in React context or `useState`, persisted with `localStorage` only for UI preferences.

## 2. Cross-cutting behaviour

### 2.1 Auth

- **Access token:** kept in memory only, never in `localStorage`.
- **Refresh:** done by the `HttpOnly` cookie through `POST /auth/refresh`. It runs silently on load and on any 401, then the request is retried once. A refresh failure sends the user to `/login` and keeps a `redirect` search param.
- **Signup:** only with an invite code (`/signup?invite=…`).
- **Email verification:** a banner until verified. Email notifications and creating invites stay disabled until then.

### 2.2 API usage

| Topic | Rule |
|---|---|
| Ids | Always the `id` (PublicId) from the API |
| Concurrency | Every edit form keeps the resource's `version` and sends `If-Match`. On `409 conflict`, show a "Changed somewhere else" dialog with *Reload* and *Overwrite* (Overwrite resends with the new version) |
| Idempotency | Create mutations send an `Idempotency-Key` (a ULID made when the form opens). Reviews, attempts and events send a `clientId` |
| Errors | Map `error.code` to plain-language messages (`lib/errors.ts`); `validation_failed.details` maps onto form fields. `404` on a detail route shows "Not found or you don't have access" (never reveal which) |
| Cache | Query keys per resource (`['course', id]`, `['mock', id]`). Mutations invalidate the narrowest keys. Optimistic updates only for flashcard ratings, mock answers and toggles |
| Time | Show every time in `me.timeZone` (date-fns-tz). Show relative dates ("in 6 days") for deadlines |
| Events | `lib/events.ts` batches user events (`material.open`, `material.page`, `notes.step`, …) and flushes every 10 s, on `visibilitychange` and on `pagehide` (`navigator.sendBeacon`) |

### 2.3 Live updates (SSE)

`lib/sse.ts`:

1. Gets a ticket (`POST /sse-ticket`) and opens an `EventSource`.
2. Reconnects with backoff (1, 2, 5, 10 s) on drop.
3. Exposes `useEventStream(url, handlers)`.

Used for:

- AI chat answers;
- import progress;
- file processing status;
- the notification inbox.

### 2.4 Content rendering

- `<Markdown>`: `react-markdown` + `remark-math` + `rehype-katex`, sanitised with `rehype-sanitize` (no raw HTML). It is used for every prompt, explanation, card, note and AI message.
- `<Math>`: KaTeX with `throwOnError: true`. When KaTeX fails, the component lazy-loads MathJax for that expression only.
- Code blocks: syntax-highlighted with Shiki (lazy-loaded).

### 2.5 AI is opt-in, everywhere

- `useAi()` returns the gate per feature and course, from `GET /ai/settings` and `GET /ai/courses/{id}/settings`, plus `reason` when off.
- Every AI control renders only when the gate is open. Otherwise:
  - **AI off for the user:** nothing appears, except one "Turn on AI features" entry in Settings.
  - **AI on but blocked here** (course off, material blocked, no key): the control appears disabled, with a tooltip giving the reason and a link to fix it.
- The non-AI app must be complete without the ai service. If `/api/v1/ai/*` returns 5xx or `service_unavailable`, hide AI controls and keep working.

### 2.6 Keyboard

- Global: `⌘/Ctrl+K` opens the command palette (go to course, lecture, question, mock; "Take me there"), `g` then `c` goes to courses, `?` shows shortcuts.
- Mocks, from the current Exam Board:
  - `1`–`9`: choose that option, confidence *sure*;
  - `Shift+1`–`9`: *unsure*;
  - `Alt/Option+1`–`9`: *guess*;
  - `Space`: next question;
  - `F`: flag.

  Use `event.code` so Shift and Alt don't change the digit.
- Flashcards: `Space` reveals; `1`–`4` rate Again, Hard, Good, Easy.

### 2.7 Accessibility (WCAG 2.2 AA)

- Every interactive element is reachable and operable by keyboard, with a visible focus ring.
- Dialogs trap focus (Radix does this).
- Right/wrong is never shown by colour alone: always a word or icon as well.
- KaTeX output includes MathML.
- Charts have a table alternative.
- Touch targets are at least 24×24 px (44 px on the mobile mock and flashcard screens).
- Respect `prefers-reduced-motion`.

## 3. Design (EMPTY: to be filled in the frontend design thread)

> Nick, 2026-10-23: "for the design, leave that for now empty, I will create a new thread with the front end step to populate it."

### 3.1 Token names (fixed now, values later)

Build every component with these CSS variables only, so the design thread can set the values without touching code:

```css
/* colour */      --bg, --surface, --surface-raised, --border, --text, --text-muted,
                  --primary, --primary-text, --danger, --warning, --success, --focus-ring,
                  --course (set per course at runtime, see 5.2)
/* type */        --font-sans, --font-mono, --text-xs .. --text-2xl
/* space/shape */ --space-1 .. --space-8, --radius-sm, --radius-md, --radius-lg
/* motion */      --duration-fast, --duration-base, --ease-standard
```

Light values go on `:root` and dark values on `[data-theme="dark"]`. The theme toggle sets `data-theme` on `<html>`; "system" follows `prefers-color-scheme`.

### 3.2 Visual language

*To be written: palette, type scale, spacing, component skins, iconography, motion, empty states, illustration.*

### 3.3 Screen layouts

*To be written: wireframes or mockups for each screen in section 4.*

## 4. Screens and routes

Layout shell for every signed-in route:

- **Left sidebar:**
  - the Crux logo;
  - **Take me there** (primary action);
  - Home;
  - the list of my courses (colour dot, short name, next exam countdown);
  - "+ New course";
  - Flashcards, Inbox (unread count), Settings.
- **On a phone:** the sidebar becomes a drawer behind a menu button, and a bottom bar holds Take me there, Courses, Flashcards and Inbox.
- **Top bar:** breadcrumbs, search (opens the command palette), theme toggle, avatar menu.

| Route | Screen | Shows / does | Main API |
|---|---|---|---|
| `/login`, `/signup?invite=`, `/forgot`, `/reset?token=`, `/verify?token=` | Auth | forms with field errors | `/auth/*` |
| `/` | **Home** | today's plan items, upcoming exams with countdown, the "Take me there" card with `reason`, per-course cards (lectures done/total, hours actual vs estimate, first-try MC rate, top 3 weak spots), cross-course recurring mistakes, unread notifications | `/plan`, `/courses`, `/courses/{id}/stats/summary`, `/insight/patterns`, `/next` |
| `/next` | **Take me there** | runs the steps from `GET /next` one at a time: the warm-up (one item at a time), then the lecture, mock or flashcards. A step bar with "Skip" and "Done"; `POST /next/advance` after each step | `/next`, `/courses/{id}/warmup`, `/attempts` |
| `/week/:isoWeek?` | **This week** | Mon–Sun planned vs actual hours per day per course, what was studied, slips per day, week totals, on-track/behind per exam; arrows to step between weeks; daily heatmap below | `/week`, `/plan` |
| `/courses/new` | **New course** | two paths: **Manual** (name, code, colour, grade scale, term) or **Set up with AI** (only with the gate open: drop PDFs, instructions, "turn past exams into questions" toggle), leading to the import review | `/courses`, `/ai/imports` |
| `/imports/:id` | **Import review** | live progress (SSE), then proposals grouped by kind (Course, Lectures, Materials, Papers, Questions, Flashcards, Deadlines, Plan entries, Exam aids). Each proposal shows source file + page (opens the PDF at that page), confidence, rationale, and duplicate warnings. Accept, edit or reject one; bulk-select; "Apply accepted" | `/ai/imports/*`, `/ai/proposals/*` |
| `/c/:courseId` | **Course overview** | exam dates and format, progress bar, hours actual vs estimate with remaining range, MC-rate trend, weak spots table (topic, lectures, misses, severity, last seen, key rule as display maths), resolved list, revision checklist, traps | `/courses/{id}`, `/courses/{id}/stats/summary`, `/courses/{id}/weak-spots`, `/courses/{id}/traps` |
| `/c/:courseId/lectures` | **Lectures** | ordered list; status (todo / in progress / done + date), steps, hours vs estimate, past-exam questions done/total; reorder (Editors) | `/courses/{id}/components`, `/courses/{id}/progress` |
| `/c/:courseId/l/:componentId` | **Lecture** | tabs: Materials, Questions, Flashcards, Slips, Sessions, Notes. The Materials tab lists PDFs and notes (Excluded badge, AI-blocked badge, "original removed" state) | `/components/{id}/materials`, … |
| `/c/:courseId/m/:materialId` | **Material viewer** | pdf.js viewer (or markdown). When the original is purged: thumbnails if kept, otherwise the extracted text per page with a banner "Original removed to save space. Re-upload to restore". Select text → floating **Ask AI** button opens the side panel with that quote as the anchor. Page-view events recorded. Embedded check questions (notes) render inline after their step | `/files/{id}/url`, `/files/{id}/pages/{n}/text`, `/ai/conversations` |
| side panel (any course page) | **Ask AI** | right-hand panel (on a phone: a full-screen sheet). Conversation anchored to the current material, lecture, question, mock or course; tutor template picker; streamed answer with clickable `[p.N]` citations that jump the viewer; thumbs up/down; history list per anchor | `/ai/conversations/*` (SSE) |
| `/c/:courseId/bank` | **Question bank** | data table with filters (lecture, type, source, paper, key reliability, review state, untested, generated) and search; badges for Generated, Draft, Disputed; bulk archive; export (Markdown/JSON) | `/courses/{id}/questions` |
| `/c/:courseId/bank/new`, `/q/:questionId` | **Question editor** | type switch (MC, Numeric, Open); prompt (markdown + maths with live preview); image (upload or "crop from a PDF page" picker); MC options with a correct toggle and distractor notes; numeric value, tolerance and unit; open model answer + rubric rows; concepts; paper + number; version history with diff | `/questions/*` |
| `/c/:courseId/papers` | **Past papers** | exam map: label, date, bank n/total, key status, what was left out and why | `/courses/{id}/papers` |
| `/c/:courseId/mocks` | **Mocks** | past mocks (score, date, status), sure-vs-unsure view per lecture, **New mock** builder: real exam layout (default on when the deadline has a layout), lectures, n, time limit, go easy, misses only, grading mode Self/AI (AI only with the gate open). Shows `pool_short` warnings | `/courses/{id}/mocks`, `/courses/{id}/stats/confidence` |
| `/mock/:mockId` | **Mock (taking)** | start screen with "Your traps" and the rules; then one question per screen, confidence buttons, flag, a question navigator grid (answered / flagged / blank), countdown (warns at 10 and 1 min; auto-submit at 0). Answers save on every change (debounced 500 ms) with a saved indicator. Leaving asks for confirmation | `/mocks/{id}/*` |
| `/mock/:mockId/result` | **Mock result + review** | score (correct/total, open counted Right 1 / Partial 0.5); per question Right/Wrong/Blank (+ confidence); key hidden until revealed; "Show answer" (marks self-shown); "I fixed this" (marks fixed); self-grading for open answers; with AI grading: each suggestion with feedback, Accept / Override (grade input) / Reject, plus "Accept all". Proposed slips with confirm/edit/dismiss; "make a flashcard" | `/mocks/{id}/result`, `/ai/grades?mockId=` |
| `/flash` and `/flash/:courseId?` | **Flashcards** | today's queue per course or all (due + new counts); review screen: front, Space to reveal, back, rate 1–4 + optional confidence, "include in next warm-up" checkbox; end-of-session summary; **Custom set** (lectures, slips / forgotten / unseen, count) | `/flash/queue`, `/flash/reviews`, `/flash/custom` |
| `/c/:courseId/cards` | **Card list** | cards with code (BSA017), source, lecture, lapses, suspended; create and edit; lookup by code | `/courses/{id}/flashcards` |
| `/c/:courseId/plan` and `/plan` | **Plan** | week calendar of plan entries (drag to move = `Moved`; resize; edit description; skip). "Rebuild plan" shows the `changeSummary`. User-edited entries show a pin | `/plan`, `/plan/entries/*`, `/plan/rebuild` |
| `/c/:courseId/exams` | **Exams** | deadlines (exam, resit, deadline) with format, pass mark, layout editor (questions per lecture, open count), check-in result form (grade on the course's scale, provisional toggle, note), "Create resit" | `/courses/{id}/deadlines`, `/deadlines/*` |
| `/c/:courseId/settings` | **Course settings** | name, code, colour (Owner), grade scale (Owner), sharing (Owner: invite by username or email as Editor/Viewer, remove), AI for this course (on/off, tutor template, nightly generation), **Storage**: per-material size and "Free up space", course-wide "Free up space", pending purges with Undo (7 days), archive/unsubscribe | `/courses/{id}`, `/courses/{id}/members`, `/ai/courses/{id}/settings`, `/materials/{id}/free-space` |
| `/insight` | **Mistakes** | slips (filter by course, type, open/resolved), weak spots across courses, cross-course patterns | `/slips`, `/insight/patterns` |
| `/inbox` | **Inbox** | in-app notifications (SSE), mark read, deep links; exam check-in prompts answerable inline | `/notify/inbox`, `/notify/stream` |
| `/settings/profile` | Profile | name, username, email (re-verify), time zone, password | `/me` |
| `/settings/study` | Study | flashcard retention, new per day, review limit; mock defaults (real layout, go easy); daily hours; one course per day | `/me/settings` |
| `/settings/ai` | **AI** | master switch with the consent text and policy version (required to turn on); per-feature toggles; data sharing (progress, mistakes, answers) with plain explanations; auto-approve generated questions; nightly generation; **API keys** (add per provider with a label, verify, revoke, set default; only the last 4 digits are ever shown); default model; **Tutor** (pick a built-in or create your own: personality, teaching style, rules, free text); monthly spending cap; usage per month and feature | `/ai/settings`, `/ai/keys`, `/ai/tutors`, `/ai/usage` |
| `/settings/notifications` | Notifications | matrix of kinds × channels (In-app, Email), all off by default; email needs a verified address | `/notify/preferences` |
| `/settings/storage` | Storage | used vs quota (tier name), per-course breakdown with purgeable bytes, auto-delete originals toggle, keep thumbnails toggle | `/files/storage` |
| `/settings/account` | Account | export my data, delete account (14-day grace, choose what happens to shared courses), my invites (create, copy, revoke) | `/account/*`, `/invites` |
| `/archived` | **Archived** | one card per passed or archived course: exam date, grade, total hours, final MC rate, weak-spot history; lectures read-only | `/courses?status=archived` |
| `/admin/*` | Admin (Role=Admin only) | users (tier change), tiers (quota), invites | `/admin/*` |

## 5. Feature details the screens must get right

### 5.1 Uploads

- `features/files`:
  1. hash the file in a Web Worker (sha256, so the UI stays responsive);
  2. `POST /files/uploads`;
  3. if a file is returned as already present, skip the upload;
  4. otherwise PUT to the signed URL with a progress bar;
  5. `POST /files/{id}/complete`;
  6. follow `/files/{id}/events` (SSE) through scanning → extracting → ready.
- Only PDFs are accepted for now (`accept="application/pdf"`; the list comes from config).
- Show quota errors with used/quota and a link to Storage.
- Re-uploading a purged original shows "Restored: your questions and notes are reconnected".

### 5.2 Course colour

- Each course has one colour (`course.color`). Inside a course route, set `--course` on the layout root.
- It is used only as a thin accent: sidebar dot, tab underline, progress bar, the dot in the Crux mark, chart series.
- Text never uses `--course` directly. Compute a readable foreground with a contrast check (`lib/color.ts`, unit-tested to ≥ 4.5:1).
- Only the course Owner sees the colour picker.

### 5.3 Mock taking (must not lose answers)

- Each answer is saved with `PUT …/answer` (`If-Match`). Unsaved answers are kept in `sessionStorage`, keyed by mock, and retried on reconnect.
- The timer uses server `startedAt` + `timeLimitMin` minus the clock offset measured from the response `Date` header, never just the local clock.
- At 0 the app submits automatically. If the submit fails, it keeps retrying and shows "Submitting…".
- Key hiding follows the API exactly: never render a key the result endpoint didn't return.

### 5.4 Flashcards

- The queue is fetched per session. Ratings post in batches of up to 20 (or every 15 s, and on leaving), each with a `clientId`; the UI advances optimistically.
- If the response flags `stale`, refetch the queue.
- End of session shows: count, again/hard/good/easy, lapsed cards, sure-wrong cards (which become slip proposals).

### 5.5 Ask AI panel

- Opening it from a selection sends `anchor {fileRef, page, quote}`. Opening it elsewhere anchors to the page's object.
- Streaming text renders through `<Markdown>`, so maths renders as it arrives.
- Citations are chips `p.12`; clicking one opens or scrolls the viewer to that page and highlights the quote.
- The panel shows what the AI saw: a collapsible "Context used" list from the run context. This is a requirement, not a nice-to-have.
- Tutor template: a picker in the panel header; the default comes from the course, then the user.

### 5.6 Import review

- Proposals are editable inline with the same forms as manual creation (reuse the editors).
- Each card shows confidence (High / Medium / Low) as a word plus an icon, and "From: file.pdf p.4".
- Low-confidence proposals are unselected by default.
- Apply is disabled until at least the Course proposal is accepted.
- After apply, go to the new course.

### 5.7 Empty, loading and error states

- Every list has an empty state that says what goes there and offers the action to add the first item.
- Loading uses skeletons, not spinners, for page content.
- Errors show inline with Retry. A global error boundary offers "Reload" and the request id from the error response.

## 6. Performance

- Route-level code splitting. pdf.js, MathJax, Shiki and the chart library are lazy-loaded.
- Initial JS ≤ 250 KB gzipped for the shell + Home.
- Lists over 100 rows are virtualised (TanStack Virtual).
- Lighthouse on Home (desktop): Performance ≥ 90, Accessibility 100.

## 7. Security

- No tokens in `localStorage` or `sessionStorage`. Sanitised markdown only; no `dangerouslySetInnerHTML` outside the Markdown and KaTeX renderers.
- CSP from the gateway:
  - `default-src 'self'`
  - `img-src 'self' blob: data: <minio host>`
  - `connect-src 'self' <minio host>`
  - `script-src 'self'`
  - `style-src 'self' 'unsafe-inline'` (required by KaTeX)
- API keys typed in Settings are sent once and cleared from component state immediately after submit.

## 8. Charts

Use one library: Recharts. It covers the hours per day stacked bars, the MC first-try trend line, the daily heatmap (a custom grid component) and the confidence bars. Every chart has a "Show as table" toggle.

## 9. Tests and definition of done

**Unit** (Vitest, ≥ 80% on `lib/` and feature `lib/`):

- error mapping;
- time-zone formatting;
- countdown with clock offset;
- key-hiding render rules;
- course-colour contrast;
- event batching;
- SSE reconnect;
- mock keyboard mapping;
- markdown/maths sanitising.

**Playwright** (against the Compose stack with `make seed`, AI via the fake provider):

1. sign up with an invite;
2. create a course manually;
3. upload a PDF;
4. add MC, numeric and open questions;
5. build a mock in the real exam layout;
6. answer with keyboard shortcuts;
7. submit;
8. self-grade the open answer;
9. reveal and fix;
10. confirm a slip;
11. make a flashcard;
12. review flashcards;
13. check Home and This week;
14. Take me there: warm-up, then the lecture;
15. turn AI on: add a key, ask about a PDF selection (citation jumps to the page), AI-grade and accept all, set up a course with AI and apply;
16. free up space on a material, undo, free again; re-upload restores;
17. a second user can't open the first user's mock (404 page).

**Accessibility:** axe checks in Playwright on every screen, with 0 serious violations.

**Done means:**

- CI green;
- the client is regenerated from OpenAPI with no diff;
- every screen in section 4 works in light and dark at 1280 px and 375 px;
- the README covers setup and env.

## 10. Build order (follows the backend milestones)

1. Shell, auth, theme toggle, sidebar, command palette, generated client, SSE helper.
2. Courses, lectures, materials, uploads, PDF viewer.
3. Bank + editors, mocks (taking, result, review), slips, weak spots, traps.
4. Flashcards.
5. Home, This week, Plan, Take me there, Exams.
6. Inbox + notification settings.
7. Storage saver UI.
8. AI: settings, keys, tutors, Ask AI panel, AI grading review, import review, generation.
9. Accessibility pass, performance budget, Playwright suite complete.
10. **Apply section 3** once the design thread has filled it: token values, component skins, screen layouts.

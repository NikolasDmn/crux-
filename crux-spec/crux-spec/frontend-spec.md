# Crux: frontend spec

> Version 1.1, 2026-10-24. Audience: an engineering agent that builds the web app from this document plus `backend-spec.md` (the API it calls) and `OPEN-QUESTIONS.md` (the backend as built: where it differs from `backend-spec.md`, that file wins). 1.1 follows the finished backend (M1–M9) and the decisions of 2026-10-23/24; changed rows say so. Section 3 (Design) was filled on 2026-10-24 from the design proposal and the widget picks.

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
| Feel | Calm and minimal, like Notion or Linear (section 3) |
| Language | English only. All strings go through one `t()` helper with English keys, so translation can be added later without touching components |
| Accessibility | WCAG 2.2 AA |
| Browsers | Latest 2 versions of Chrome, Firefox, Safari and Edge; iOS Safari and Android Chrome |
| Tests | Vitest + Testing Library (80% line coverage on `lib/` logic and hooks); Playwright for the main flows (section 9) |
| Offline / PWA | Later. Keep `clientId` on reviews, attempts and events so offline sync can be added without API changes |
| Name / logo | Crux; mark A from `brand/crux-mark-{light,dark}.svg`. The dot takes the current course's colour (section 5.2) |

### 0.1 Decisions (Nick, 2026-10-24)

| Topic | Decision |
|---|---|
| Course invitations | The always-in-app inbox notice has **Accept / Decline** inline, and pending invitations sit at the top of the sidebar's course list until answered (`GET /course-invitations`, `POST /course-invitations/{courseId}/accept` / `decline`) |
| Trash | A quiet **Trash** link under the sidebar's course list opens `/trash`: each trashed course with days left of 30 and **Restore** |
| Question-bank export | Built in the browser from the filtered list on screen (Markdown or JSON). No backend endpoint |
| Logged out | `/` goes to `/login`, which links "Have an invite? Sign up". No landing page |
| Phone | As specced: mocks, flashcards, Take me there, inbox and the material viewer work fully at 360 px; the rest works on a phone but is designed for desktop |
| Demo console | Kept at `/demo` in dev only (`CRUX_ENV=dev`); the app takes over `/` |
| First run | A user with no courses sees one guided card on Home: create the first course (manually, or from PDFs when AI is on), then add an exam date, then upload slides |
| Plan editing | Drag to move (pins the entry) and drag an edge to resize on desktop; on a phone, edit in a sheet |

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
- **Session length:** a login lasts 30 days, sliding. Two tabs refreshing at once is fine (10 s grace); a refresh that comes back 401 after logout elsewhere just goes to `/login`.
- **Signup:** only with an invite code (`/signup?invite=…`; a course-share invite also accepts the share).
- **Breached passwords** (signup, reset, change): a field error `breached` is a warning, not a block. Show "This password appeared in a data breach. Choose another, or use it anyway", and resend with `acceptBreachedPassword: true` if the user insists. Length rules are never skippable.
- **Email verification:** a banner until verified. Email notifications, creating invites (`403 email_unverified`) and sharing by email stay disabled until then.
- **Pending deletion:** a user whose account is scheduled for deletion can still log in; every page shows a banner with the date and **Cancel deletion** (`POST /account/delete/cancel`).

### 2.2 API usage

| Topic | Rule |
|---|---|
| Ids | Always the `id` (PublicId) from the API |
| Concurrency | Every edit form keeps the resource's `version` and sends `If-Match`. On `409 conflict`, show a "Changed somewhere else" dialog with *Reload* and *Overwrite* (Overwrite resends with the new version) |
| Idempotency | Create mutations send an `Idempotency-Key` (a ULID made when the form opens). Reviews, attempts and events send a `clientId` |
| Errors | Map `error.code` to plain-language messages (`lib/errors.ts`); `validation_failed.details.fields` maps onto form fields. `404` on a detail route shows "Not found or you don't have access" (never reveal which). `429 rate_limited` shows "Too many attempts" with `details.retryAfterSecs` as a countdown (login, signup, AI chat: 30 messages a minute) |
| Cache | Query keys per resource (`['course', id]`, `['mock', id]`). Mutations invalidate the narrowest keys. Optimistic updates only for flashcard ratings, mock answers and toggles |
| Time | Show every time in `me.timeZone` (date-fns-tz). Show relative dates ("in 6 days") for deadlines |
| Events | `lib/events.ts` batches user events (`material.open`, `material.page`, `notes.step`, …) and flushes every 10 s, on `visibilitychange` and on `pagehide` (`navigator.sendBeacon`) |

### 2.3 Live updates (SSE)

Two mechanisms (changed in 1.1):

1. **`EventSource` streams** (`lib/sse.ts`): get a ticket (`POST /sse-ticket`, single use, 60 s), open the `EventSource` with `?ticket=`, reconnect with backoff (1, 2, 5, 10 s) and a fresh ticket on drop. Exposes `useEventStream(url, handlers)`. Used for:
    - the notification inbox: `GET /notify/stream` (`ready`, then `notification` per item, `ping` every 20 s);
    - file processing status: `GET /files/{id}/events`;
    - import progress: `GET /ai/imports/{id}/events`.
2. **The AI chat reply** is the response body of `POST /ai/conversations/{id}/messages`, read with `fetch` and a stream reader (`lib/stream.ts`), because `EventSource` can't POST. Events: `delta {text}`, `citation {fileId, page, chunkNo, quote}`, then `done {messageId, costMicroUsd, servedBy, contextUsed}` or `error {code, message}`. Gate, budget and rate-limit errors arrive as ordinary HTTP errors before the stream starts.

### 2.4 Content rendering

- `<Markdown>`: `react-markdown` + `remark-math` + `rehype-katex`, sanitised with `rehype-sanitize` (no raw HTML). It is used for every prompt, explanation, card, note and AI message.
- `<Math>`: KaTeX with `throwOnError: true`. When KaTeX fails, the component lazy-loads MathJax for that expression only.
- Code blocks: syntax-highlighted with Shiki (lazy-loaded).

### 2.5 AI is opt-in, everywhere

- `useAi(feature, courseId?)` asks core `GET /me/ai-allowed?feature=&courseId=` (changed in 1.1), cached per feature and course and invalidated when AI settings change. It returns `allowed` plus `reason` when off: `user_off`, `feature_off`, `course_off`, `material_blocked`, `no_key`, `service_unavailable`. The settings screens still read `GET /ai/settings` and `GET /ai/courses/{id}/settings`.
- Every AI control renders only when the gate is open. Otherwise:
  - **AI off for the user:** nothing appears, except one "Turn on AI features" entry in Settings.
  - **AI on but blocked here** (course off, material blocked, no key): the control appears disabled, with a tooltip giving the reason and a link to fix it.
- The non-AI app must be complete without the ai service. If `/api/v1/ai/*` returns 5xx or the gate says `service_unavailable`, hide AI controls and keep working.

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

## 3. Design (Nick, 2026-10-24)

**Ink on warm paper.** The grounds come from the mark (`#fafaf9` / `#161618`), text and primary controls are ink, and colour appears only as a thin accent per course. Calm and dense like Linear: 14 px body, 1 px borders, no shadows on resting surfaces. Reference artboards: the [design canvas](https://claude.ai/artifact/X8pYXTTwxkHCjjNkFgoP79) (foundations, components, Home, mock and flashcards on a phone); the widget choices below were picked in the [widget playground](https://claude.ai/artifact/WciAeo2MAQbsTFUGtL9CNU).

### 3.1 Tokens

Build every component with these CSS variables only. Light values on `:root`, dark on `[data-theme="dark"]` (the toggle sets `data-theme` on `<html>`; "system" follows `prefers-color-scheme`, so the dark values are also applied under `@media (prefers-color-scheme: dark) { :root:not([data-theme="light"]) }`).

| Token | Light | Dark | Use |
|---|---|---|---|
| `--bg` | `#fafaf9` | `#161618` | page ground |
| `--surface` | `#ffffff` | `#1c1c1f` | cards, inputs, panels |
| `--surface-sunken` | `#efeeea` | `#232327` | selected nav row, tracks, chips |
| `--surface-raised` | `#ffffff` | `#232327` | popovers, dialogs, toasts (with `--shadow`) |
| `--border` | `#e6e5e1` | `#2e2e33` | every 1 px border and divider |
| `--text` | `#1c1c1a` | `#f2f2ef` | body text |
| `--text-muted` | `#6b6a65` | `#a3a29d` | secondary text (≥ 4.5:1 on `--bg` and `--surface`) |
| `--primary` | `#1c1c1a` | `#f2f2ef` | primary buttons, selected states (ink) |
| `--primary-text` | `#fafaf9` | `#161618` | text on `--primary` |
| `--danger`, `--danger-bg`, `--danger-text` | `#c4321f`, `#fdecea`, `#a8281a` | `#ff6b5b`, `#3a1b17`, `#ff8a7d` | wrong, destructive, errors |
| `--warning`, `--warning-bg`, `--warning-text` | `#a15c00`, `#fdf3e3`, `#8a4b00` | `#f0b45a`, `#3a2a12`, `#f0b45a` | short, breached, pending |
| `--success`, `--success-bg`, `--success-text` | `#1f7a4d`, `#e7f4ec`, `#17603c` | `#4cc38a`, `#13301f`, `#6fd6a3` | right, done |
| `--info-bg`, `--info-text` | `#e9edfb`, `#2a47b8` | `#1b2240`, `#9db1ff` | duplicates, neutral notices |
| `--focus-ring` | `#c4520a` | `#ff8a3d` | 2 px outline, 2 px offset, on `:focus-visible` |
| `--course`, `--course-soft` | per course (below) | per course | the course accent and its tint |
| `--shadow` | `0 8px 24px rgb(28 28 26 / .10)` | `0 8px 24px rgb(0 0 0 / .45)` | raised layers only |

**Course colours** (`Course.Color`; the picker offers these eight, default Orange):

| Name | `--course` | `--course-soft` light / dark |
|---|---|---|
| Orange | `#f76b15` | `#fde7d9` / `#3a2416` |
| Blue | `#3e63dd` | `#e1e7fb` / `#1b2240` |
| Teal | `#12a594` | `#d9f2ee` / `#10302c` |
| Purple | `#8e4ec6` | `#ede1f7` / `#2a1d38` |
| Pink | `#e5559f` | `#fae0ef` / `#3a1a2e` |
| Green | `#30a46c` | `#dcf2e5` / `#13301f` |
| Amber | `#e2a336` | `#fbefd6` / `#36290f` |
| Slate | `#8b8d98` | `#ececee` / `#26272b` |

`--course` is set on the course layout root (5.2). It is used only for the sidebar dot, the tab underline, progress fills, plan block edges, chart series and the dot in the mark; text never uses it (`lib/color.ts` picks `#1c1c1a` or `#ffffff` for anything drawn on it, by contrast).

```css
/* type */        --font-sans: Geist, ui-sans-serif, system-ui, sans-serif;
                  --font-mono: "Geist Mono", ui-monospace, monospace;
                  --text-xs 12px · --text-sm 13px · --text-base 14px · --text-md 16px · --text-lg 20px · --text-xl 24px · --text-2xl 32px
/* space */       --space-1 4px · --space-2 8px · --space-3 12px · --space-4 16px · --space-5 20px · --space-6 24px · --space-7 32px · --space-8 48px
/* shape */       --radius-sm 4px · --radius-md 8px · --radius-lg 12px (cards); pills 999px
/* motion */      --duration-fast 120ms · --duration-base 200ms · --ease-standard cubic-bezier(0.2, 0, 0, 1)
```

### 3.2 Visual language

- **Type:** Geist 400/500/600 (bundled from `@fontsource-variable/geist` and `@fontsource-variable/geist-mono`; no font CDN, so the CSP stays `self`), Geist Mono for card codes, times, hours, scores and countdowns (`font-variant-numeric: tabular-nums`). Body 14 px / 1.5; headings 600 with −0.01 to −0.02 em tracking and `text-wrap: balance`; section labels 12 px 500 uppercase, +0.06 em, muted. On phone screens (mocks, flashcards) question and card text is 16–22 px.
- **Surfaces:** cards are `--surface` with a 1 px `--border` and `--radius-lg` (picked: *Border*). Only popovers, dialogs, the command palette and toasts are raised (`--surface-raised` + `--shadow`). Nothing else gets a shadow, gradient or coloured side rail.
- **Icons:** Lucide (`lucide-react`), 16 px, stroke 1.75, `currentColor`. No emoji anywhere.
- **Density:** controls 36 px high (32 px in toolbars and tables), touch targets 44 px on the phone screens; sidebar 248 px; content max width 1200 px with 40 px side padding (16 px on a phone).
- **Motion:** state changes fade or slide over `--duration-fast`; panels and dialogs over `--duration-base`; nothing animates on page load; everything respects `prefers-reduced-motion`.
- **Copy:** plain, short, the user's words ("Free up space", "Take me there"); numbers with units ("6 h short", "exam in 10 d").

### 3.3 Component skins (picked 2026-10-24)

| Widget | Skin |
|---|---|
| Primary button | **Ink:** `--primary` fill, `--primary-text`, 36 px, `--radius-md`. Secondary: `--surface` with `--border`. Ghost: transparent. Destructive: `--danger` fill, white text, only for the final confirm |
| Card | **Border:** `--surface`, 1 px `--border`, `--radius-lg`, 20 px padding |
| Sidebar course item | **Dot + countdown:** 8 px `--course` dot, course code, mono countdown to the next exam right-aligned and muted; the current course row on `--surface-sunken` |
| Progress | **Thin bar:** 6 px track on `--surface-sunken`, `--course` fill, a label row above (count left, mono hours right) |
| Multiple-choice option | **Row with key:** full-width row, 1 px `--border`, a 22–26 px key square showing the shortcut digit; selected = 2 px `--text` border and an ink key |
| Confidence | **Segmented control:** one bordered group of Sure / Unsure / Guess, the selected segment on `--surface-sunken` (ink on the phone mock screen for contrast) |
| Flashcard rating | **Four with intervals**, each with its own icon and a soft semantic tint so the feeling reads at a glance (Nick's note): Again `rotate-ccw` on `--danger-bg`/`--danger-text`, Hard `trending-down` on `--warning-bg`/`--warning-text`, Good `check` on `--success-bg`/`--success-text`, Easy `zap` on `--info-bg`/`--info-text`; the next interval in mono under the label; keys 1–4. Same shape and weight for all four |
| Badges | **Square tags:** `--radius-sm`, 22 px, 11–12 px 500; status words uppercase with +0.04 em (`DRAFT`, `DUPLICATE`, `GENERATED`), codes in mono (`BSA017`); tints from the semantic tokens (`--info-*` for duplicates, `--warning-*` for disputed, `--surface-sunken` otherwise) |
| Undo toast | **Ink toast:** bottom-centre (bottom-left on desktop), `--primary` ground, `--primary-text`, an outlined Undo button in the same colour, `--shadow`; 6 s, paused on hover and focus |
| Empty states | Three patterns, each for its own place (Nick: "I could say yes to all of these"): **text + action** for empty lists (one line on what goes there, one button); **dashed drop box** wherever files can be added (materials, imports, papers); **first-run checklist** on Home for a new user (create the course, add the exam date, upload the slides; each step links to the screen that does it and ticks itself when done) |
| Plan block | **Colour edge:** `--surface` block with a 3 px `--course` left edge and 1 px `--border` on the other sides; mono time range above the title; pinned entries show a pin icon |
| Weak-spot severity | **Bar + word:** the word (Low / Medium / High) right-aligned, a 4 px bar under the topic, `--danger` for High, `--warning` for Medium, `--text-muted` for Low |
| Banners | full-width row on `--surface` with a 1 px border and a status dot (verify email), or on `--danger-bg` / `--warning-bg` with the matching text token (pending deletion, "X h short") |
| Inputs | 36 px, `--surface`, 1 px `--border`, `--radius-md`; error = `--danger` border + message below in `--danger-text`; the breached-password warning uses the warning tokens and an inline "Use it anyway" link |
| Tabs | text tabs with a 2 px `--course` underline on the active one (outside a course: `--text`) |

### 3.4 Screen layouts

- **Shell:** the 248 px sidebar (mark, Take me there as an ink button, Home, This week, pending invitations as small cards with Accept / Decline, the course list, "+ New course", a muted Trash link, then Flashcards with the due count, Inbox with an ink unread count, Settings at the bottom); the top bar holds breadcrumbs, search (opens the palette, shows `⌘K`), the theme toggle and the avatar menu.
- **Home:** greeting + date; a two-column-wide ink **Take me there** card (next steps in one sentence, Start); the check-in card beside it; banners ("X h short"); Today and Exams side by side; course cards in an auto-fit grid (min 280 px); the recurring-mistakes row. A new user sees the first-run checklist instead.
- **Mock (phone):** header with close, course and "Question n of N", a mono countdown pill; a 3 px `--course` progress line; question text 17 px; option rows with keys; the confidence control; a footer with Flag, All questions and an ink Next.
- **Flashcards (phone):** header with the remaining count; a segmented progress line (`--course` for done, ink for current); the card fills the screen (front, then the answer under a divider); Show answer as one ink button, then the four rating buttons; "Include in my next warm-up" below.
- Other screens follow the same parts: section headers + bordered cards, data tables with 32 px rows and sticky headers, side panels (Ask AI) at 420 px.

## 4. Screens and routes

Layout shell for every signed-in route:

- **Left sidebar:**
  - the Crux logo;
  - **Take me there** (primary action);
  - Home;
  - pending course invitations (if any), each with Accept / Decline (0.1);
  - the list of my courses (colour dot, short name, next exam countdown);
  - "+ New course", then a quiet "Trash" link;
  - Flashcards, Inbox (unread count), Settings.
- **On a phone:** the sidebar becomes a drawer behind a menu button, and a bottom bar holds Take me there, Courses, Flashcards and Inbox.
- **Top bar:** breadcrumbs, search (opens the command palette), theme toggle, avatar menu.

| Route | Screen | Shows / does | Main API |
|---|---|---|---|
| `/login`, `/signup?invite=`, `/forgot`, `/reset?token=`, `/verify?token=` | Auth | forms with field errors; the breached-password warning (2.1); signed-out `/` redirects to `/login` | `/auth/*` |
| `/` | **Home** | first run (no courses): the guided card (0.1). Otherwise today's plan items, upcoming exams with countdown, the "Take me there" card with `reason`, **exam check-ins** waiting for a result (from `/next`'s `checkIns`, answered inline), "X h short" warnings (`GET /plan/short`), per-course cards (lectures done/total, hours actual vs estimate, first-try MC rate, top 3 weak spots), cross-course recurring mistakes, unread notifications | `/plan`, `/plan/short`, `/courses`, `/courses/{id}/stats/summary`, `/insight/patterns`, `/next` |
| `/next` | **Take me there** | runs the steps from `GET /next` one at a time: the warm-up (at most 3 items, one at a time), then the lecture, mock or flashcards. A step bar with "Skip" and "Done"; `POST /next/advance` (stateless: send the step's `kind`, target, `planEntryId`, `skipped`) after each step | `/next`, `/courses/{id}/warmup`, `/attempts` |
| `/week/:isoWeek?` | **This week** | Mon–Sun planned vs actual hours per day per course, what was studied, slips per day, week totals, on-track/behind per exam; arrows to step between weeks; daily heatmap below | `/week`, `/plan` |
| `/courses/new` | **New course** | two paths: **Manual** (name, code, colour, grade scale, term) or **Set up with AI** (only with the gate open: drop PDFs, instructions, "turn past exams into questions" toggle), leading to the import review | `/courses`, `/ai/imports` |
| `/imports/:id` | **Import review** | live progress (SSE), then proposals grouped by kind (Course, Lectures, Materials, Papers, Questions, Flashcards, Deadlines, Plan entries, Exam aids). Each proposal shows source file + page (opens the PDF at that page), confidence, rationale, and a **duplicate badge** when `duplicateOfId` is set ("Matches your L3 Variance", linked). A warning when `truncated` ("Only the first part of your files was read"). Accept, edit or reject one; bulk-select; "Apply accepted" | `/ai/imports/*`, `/ai/proposals/*` |
| `/c/:courseId` | **Course overview** | exam dates and format, progress bar, hours actual vs estimate with remaining range, MC-rate trend, weak spots table (topic, lectures, misses, severity, last seen, key rule as display maths), resolved list, revision checklist, traps | `/courses/{id}`, `/courses/{id}/stats/summary`, `/courses/{id}/weak-spots`, `/courses/{id}/traps` |
| `/c/:courseId/lectures` | **Lectures** | ordered list; status (todo / in progress / done + date), steps, hours vs estimate, past-exam questions done/total; reorder (Editors) | `/courses/{id}/components`, `/courses/{id}/progress` |
| `/c/:courseId/l/:componentId` | **Lecture** | tabs: Materials, Questions, Flashcards, Slips, Sessions, Notes. The Materials tab lists PDFs and notes (Excluded badge, AI-blocked badge, "original removed" state) | `/components/{id}/materials`, … |
| `/c/:courseId/m/:materialId` | **Material viewer** | pdf.js viewer (or markdown). When the original is purged: thumbnails if kept, otherwise the extracted text per page with a banner "Original removed to save space. Re-upload to restore". Select text → floating **Ask AI** button opens the side panel with that quote as the anchor. Page-view events recorded. Embedded check questions (notes) render inline after their step | `/files/{id}/url`, `/files/{id}/pages/{n}/text`, `/ai/conversations` |
| side panel (any course page) | **Ask AI** | right-hand panel (on a phone: a full-screen sheet). Conversation anchored to the current material, lecture, question, mock or course; tutor template picker; streamed answer with clickable `[p.N]` citations that jump the viewer; "Context used" (5.5); "Answered by <model>" when `servedBy` is set; thumbs up/down; history list per anchor | `/ai/conversations/*` (streamed POST, 2.3) |
| `/c/:courseId/bank` | **Question bank** | data table with filters (lecture, type, source, paper, key reliability, review state, untested, generated) and search; badges for Generated, Draft, Disputed; Editors see and approve the bank's drafts; bulk archive; export of the filtered list (Markdown/JSON, built in the browser, 0.1); "Generate questions" (AI gate) | `/courses/{id}/questions`, `/ai/generate` |
| `/c/:courseId/bank/new`, `/q/:questionId` | **Question editor** | type switch (MC, Numeric, Open); prompt (markdown + maths with live preview); image (upload or "crop from a PDF page" picker); MC options with a correct toggle and distractor notes; numeric value, tolerance and unit; open model answer + rubric rows; concepts; paper + number; version history with diff | `/questions/*` |
| `/c/:courseId/papers` | **Past papers** | exam map: label, date, bank n/total, key status, what was left out and why; "Extract questions" on a paper with a file (AI gate, opens the import review) | `/courses/{id}/papers`, `/ai/papers/{id}/extract` |
| `/c/:courseId/mocks` | **Mocks** | past mocks (score, date, status), sure-vs-unsure view per lecture, **New mock** builder: real exam layout (default on when the deadline has a layout), lectures, n, time limit, go easy, misses only, grading mode Self/AI (AI only with the gate open). Shows `pool_short` warnings | `/courses/{id}/mocks`, `/courses/{id}/stats/confidence` |
| `/mock/:mockId` | **Mock (taking)** | start screen with "Your traps" and the rules; then one question per screen, confidence buttons, flag, a question navigator grid (answered / flagged / blank), countdown (warns at 10 and 1 min; auto-submit at 0). Answers save on every change (debounced 500 ms) with a saved indicator. Leaving asks for confirmation | `/mocks/{id}/*` |
| `/mock/:mockId/result` | **Mock result + review** | score (correct/total, open counted Right 1 / Partial 0.5); per question Right/Wrong/Blank (+ confidence; a guessed right counts as a miss); right answers show their key at once, misses stay hidden until revealed; "Show answer" (marks self-shown); "I fixed this" (marks fixed); self-grading for open answers (also while AI suggestions are pending); with AI grading: each suggestion with feedback, Accept / Override (grade input) / Reject, plus "Accept all". Proposed slips (`slipProposed`) with confirm/edit/dismiss; "make a flashcard" | `/mocks/{id}/result`, `/mocks/{id}/questions/{order}/ai-grade/*`, `/mocks/{id}/ai-grades/accept-all`, `/ai/grades?mockId=` |
| `/flash` and `/flash/:courseId?` | **Flashcards** | today's queue per course or all (due + new counts); review screen: front, Space to reveal, back, rate 1–4 + optional confidence, "include in next warm-up" checkbox; end-of-session summary; **Custom set** (lectures, slips / forgotten / unseen, count) | `/flash/queue`, `/flash/reviews`, `/flash/custom` |
| `/c/:courseId/cards` | **Card list** | cards with code (BSA017), source, lecture, lapses, suspended; create, edit, suspend; personal vs course cards; lookup by code | `/courses/{id}/flashcards`, `/flashcards/{id}`, `/flash/cards?code=` |
| `/c/:courseId/plan` and `/plan` | **Plan** | week calendar of plan entries inside the user's study window: drag to move (old entry becomes `Moved`, new one pinned), drag an edge to resize, edit description, mark done or skip; "+ Add" your own entry; deleting a planner entry skips it and keeps it skipped. Timed mock and revision days before each exam are marked. "Rebuild plan" shows `changeSummary`; "X h short" warnings from `short`. Pinned and user entries show a pin | `/plan`, `/plan/short`, `/plan/entries/*`, `/plan/rebuild` |
| `/c/:courseId/exams` | **Exams** | deadlines (exam, resit, deadline) with format, pass mark, layout editor (questions per lecture, open count), check-in result form (grade on the course's scale, provisional toggle, note; a pass marks the course Passed, a fail offers a resit), "Create resit" | `/courses/{id}/deadlines`, `/deadlines/*` |
| `/c/:courseId/settings` | **Course settings** | name, code, colour (Owner), grade scale (Owner), sharing (Owner: invite by username or email as Editor/Viewer; pending email shares; remove a member or withdraw a share; a member can leave), AI for this course (on/off, tutor template, nightly generation), **Storage**: per-material size and "Free up space", course-wide "Free up space" (includes past papers), pending purges with Undo (7 days), archive/unsubscribe, delete (to Trash) | `/courses/{id}`, `/courses/{id}/members/*`, `/ai/courses/{id}/settings`, `/materials/{id}/free-space`, `/materials/{id}/free-space/cancel`, `/courses/{id}/free-space` |
| `/insight` | **Mistakes** | slips (filter by course, type, open/resolved), weak spots across courses, cross-course patterns | `/slips`, `/insight/patterns` |
| `/inbox` | **Inbox** | delivered in-app notifications (live over SSE; a reminder for tomorrow isn't here yet), cursor paging, unread count, mark one / all read, deep links; inline actions: exam check-in, course invitation Accept / Decline, "Your export is ready" download | `/notify/inbox`, `/notify/{id}/read`, `/notify/read-all`, `/notify/stream` |
| `/settings/profile` | Profile | name, username, email (re-verify), time zone, password | `/me` |
| `/settings/study` | Study | flashcard retention, new per day, review limit; mock defaults (real layout, go easy); **study window** (start and end, 15-minute steps, at least 1 h, default 09:00–22:00; `extra.studyStart` / `studyEnd`), daily hours, one course per day. Changing a planner setting rebuilds the plan at once | `/me/settings` |
| `/settings/ai` | **AI** | master switch with the consent text and policy version (required to turn on); per-feature toggles; data sharing (progress, mistakes, answers) with plain explanations; auto-approve generated questions; nightly generation; **API keys** (Anthropic, OpenAI, Google, Mistral; add with a label, verify, revoke, set default; only the last 4 digits are ever shown); default model with its price per million tokens; **Tutor** (pick a built-in or create your own: personality, teaching style, rules within fixed ranges, free text up to 2,000 characters); monthly spending cap; usage per month and feature | `/ai/settings`, `/ai/keys`, `/ai/tutors`, `/ai/usage` |
| `/settings/notifications` | Notifications | matrix of kinds × channels (In-app, Email), all off by default; always-in-app kinds (course shared, exam check-in, infected upload, export ready) show locked on; email needs a verified address and shows a "suppressed" state after a bounce or unsubscribe. Turning a kind on doesn't bring back past items | `/notify/preferences` |
| `/settings/storage` | Storage | used vs quota (tier name), per-course breakdown with purgeable bytes, auto-delete originals toggle, keep thumbnails toggle | `/files/storage` |
| `/settings/account` | Account | **Export**: "Export my data", the list of exports (Queued / Building / Ready with a download link valid 24 h / Expired / Failed with "Try again"). **Delete account**: password, then for each course shared with others: transfer to a member (picker) or delete; shows the date it runs (14 days) and Cancel. **My invites** (create, copy, revoke; emailed when an address is given) | `/account/export`, `/account/exports`, `/account/delete`, `/account/deletion`, `/account/delete/cancel`, `/invites`, `/files/{id}/url` |
| `/trash` | **Trash** | trashed courses (Owner): name, trashed date, days left of 30, Restore | `/courses?trashed=true`, `/courses/{id}/restore` |
| `/archived` | **Archived** | one card per passed or archived course: exam date, grade, total hours, final MC rate, weak-spot history; lectures read-only | `/courses` filtered by `mySubscription` (Passed / Archived): there is no `status` filter |
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
- Re-uploading a purged original shows "Restored: your questions and notes are reconnected" (identical bytes restore without a new scan).
- An upload that turns out infected shows the always-in-app notice and the file's `Infected` state; nothing else of it is usable.

### 5.2 Course colour

- Each course has one colour (`course.color`). Inside a course route, set `--course` on the layout root.
- It is used only as a thin accent: sidebar dot, tab underline, progress bar, the dot in the Crux mark, chart series.
- Text never uses `--course` directly. Compute a readable foreground with a contrast check (`lib/color.ts`, unit-tested to ≥ 4.5:1).
- Only the course Owner sees the colour picker.

### 5.3 Mock taking (must not lose answers)

- Each answer is saved with `PUT …/answer` (no `If-Match`: the last save wins per item, B3-12). Unsaved answers are kept in `sessionStorage`, keyed by mock, and retried on reconnect.
- The timer uses server `startedAt` + `timeLimitMin` minus the clock offset measured from the response `Date` header, never just the local clock.
- At 0 the app submits automatically. If the submit fails, it keeps retrying and shows "Submitting…".
- Key hiding follows the API exactly: never render a key the result endpoint didn't return.

### 5.4 Flashcards

- The queue is fetched per session. Ratings post in batches of up to 20 (or every 15 s, and on leaving), each with a `clientId`; the UI advances optimistically.
- Each review comes back `applied`, `stale`, `duplicate` or `unknown`; on any `stale`, refetch the queue.
- End of session shows: count, again/hard/good/easy, lapsed cards, sure-wrong cards (which become slip proposals).

### 5.5 Ask AI panel

- Opening it from a selection sends `anchor {fileRef, page, quote}`. Opening it elsewhere anchors to the page's object.
- Streaming text renders through `<Markdown>`, so maths renders as it arrives.
- Citations are chips `p.12`; clicking one opens or scrolls the viewer to that page and highlights the quote.
- The panel shows what the AI saw: a collapsible "Context used" list from `contextUsed` (on `done` and on each stored reply): the material pages with a quote (each opens the viewer there), the weak spots, slips and traps it was given, the question, and which of your data scopes were shared. This is a requirement, not a nice-to-have.
- When `servedBy` is set, a small "Answered by <model>" note sits under the reply (a refusal fallback model wrote it).
- The chat limit (30 messages a minute) shows as a short countdown, not an error page.
- Tutor template: a picker in the panel header; the default comes from the course, then the user.

### 5.6 Import review

- Proposals are editable inline with the same forms as manual creation (reuse the editors).
- Each card shows confidence (High / Medium / Low) as a word plus an icon, and "From: file.pdf p.4".
- Low-confidence proposals and question/flashcard duplicates are unselected by default. A lecture, concept or paper duplicate stays selected: accepting it links to the existing row, so its children land there.
- When creating a new course, Apply is disabled until the Course proposal is accepted (importing into an existing course has none).
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
  - `img-src 'self' blob: data:`
  - `connect-src 'self'` (uploads and downloads go through the gateway at `/crux-files`, B2-1)
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
17. a second user can't open the first user's mock (404 page);
18. share a course by username: the other user accepts from the sidebar strip;
19. trash a course and restore it from `/trash`;
20. study window: set 18:00–21:00 and see the plan move;
21. import into an existing course: a duplicate lecture shows its badge and links on apply;
22. export my data and download the zip; schedule deletion, see the banner, cancel it.

**Accessibility:** axe checks in Playwright on every screen, with 0 serious violations.

**Done means:**

- CI green;
- the client is regenerated from OpenAPI with no diff;
- every screen in section 4 works in light and dark at 1280 px and 375 px;
- the README covers setup and env.

## 10. Build order

The backend is complete (M1–M9), so the order follows a user's first week rather than the backend milestones (changed in 1.1). Section 3 is applied from the start: tokens and component skins come first.

1. Tokens and component skins (section 3), shell, auth (incl. breached warning, pending-deletion banner), theme toggle, sidebar (invitations strip, Trash link), command palette, generated client, `lib/sse.ts` and `lib/stream.ts`.
2. Courses (new, settings, sharing, trash), lectures, materials, uploads, PDF viewer; the first-run card.
3. Exams and deadlines, Plan, Home, Take me there, This week, check-ins.
4. Bank + editors, mocks (taking, result, review), slips, weak spots, traps.
5. Flashcards.
6. Inbox, notification settings, account (export, deletion, invites).
7. Storage saver UI.
8. AI: settings, keys, tutors, Ask AI panel, AI grading review, import review, generation.
9. Accessibility pass, performance budget, Playwright suite complete.

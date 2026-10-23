# Exam Board schema: review of v3, changes in v4

2026-10-23. Reviewed: `schema-v3.sql`. Result: `schema-v4.sql` (54 tables, loads clean on MySQL 8.0.46, smoke test in `schema-v4-smoke-test.sql`).

## 1. Verdict

v3 was a good model of **one student using the board**. It was not yet safe as a **multi-user product**. The review found:

- 4 bugs that would leak or lose data once a second user exists.
- About 12 integrity holes that let impossible rows in.
- No offline or concurrency story, although the board already depends on one: every write today goes through `if_version`.
- An AI layer that logged runs but could not enforce a budget, prove consent, or keep a file away from the model.

v4 fixes all of these. What remains for the app layer is listed in section 6, so nothing is left as "we'll remember".

As a baseline it stores everything the current board does (section 7 maps it). It also adds the pieces that are expensive to retrofit later: public ids, client ids, row versions, versioned questions and a partition-ready event log. A table you add later is cheap. A column that every existing row needs, or a change of key, is not. v4 front-loads the second kind.

## 2. Access model (who can see what)

MySQL has no row-level security, so this rule lives in the repository layer, and every query on user data goes through it.

| Data | Readable by | Writable by |
|---|---|---|
| Course catalog (components, materials, papers, concepts) | owner, `CourseMember` (Editor/Viewer), anyone if `Visibility = 'Public'` | owner, Editors |
| `Question` / `Flashcard` with `OwnerUserId IS NULL` | same as the course | owner, Editors |
| `Question` / `Flashcard` with `OwnerUserId = me` | only me | only me |
| Everything with a `UserId` (progress, mocks, slips, weak spots, AI chats, events) | only that user (+ Admin under audit) | only that user |

- **Sharing a course never shares progress.** A Viewer gets the bank, not your mistakes.
- **Mocks (Nick, 2026-10-23).** You can open a mock only if both hold:
  1. You hold an active `CourseSubscription` to `MockExam.CourseId`, or you are its owner or a member.
  2. You have the authority to see that mock: it is yours (`MockExam.UserId = you`). Later this can also come from an explicit grant, such as a share or a teacher role, and every grant writes a `UserEvent`.

  A subscription alone never shows other subscribers' mocks. The check runs on every request, list queries included. A failed check returns 404, not 403, so it doesn't confirm the mock exists.
- **Questions the AI generates from your weak spots start private** (`OwnerUserId = you`). They are promoted to the course bank only by an explicit action. In v3 they went straight into the shared bank, which leaked one user's weak spots to everyone and polluted the bank.
- **Forking** (`OriginCourseId`) is how a student copies a public course and edits freely without touching the original.
- **Admin access** to user data writes a `UserEvent` with `ActorUserId` set.

## 3. Findings in v3 and the fix in v4

### Critical: would leak or lose data

| # | v3 problem | Concrete failure | v4 fix |
|---|---|---|---|
| C1 | `Course.OwnerUserId ... ON DELETE SET NULL`, while `NULL` meant "platform catalog course" | Deleting a user turns their **private** course into a **global** one | Owner is `NOT NULL`, `ON DELETE RESTRICT`; the deletion job transfers or removes courses first. Catalog courses are owned by an admin account |
| C2 | `Flashcard.Code` unique **globally** | The second user to create a BSA course can't create `BSA001` | `UNIQUE (CourseId, Code)`; `Flashcard.CourseId` added (tied to the component by composite FK) |
| C3 | Generated questions had no owner | One user's weak-spot questions appear in every subscriber's bank | `Question.OwnerUserId` (NULL = course bank) |
| C4 | Ids were auto-increment only | The access check in section 2 is the guard. Sequential ids still turn one missed check into a crawl of every user's mocks, and they reveal how many mocks exist | `PublicId CHAR(26)` (ULID) on every row the API addresses; `Id` never leaves the server. This is defence in depth, not a replacement for the check |

### High: impossible rows, or a feature that can't work

| # | v3 problem | v4 fix |
|---|---|---|
| H1 | An MC version could get an `OpenQuestion` key row, or both | `Type` is repeated on `QuestionVersion` and on each key table, with composite FK `(Id, Type)` plus `CHECK (Type = 'MC')`. Tested: it rejects the mismatch |
| H2 | `MockQuestion.QuestionVersionId` could belong to a different question than `QuestionId` (it was an app rule) | Composite FK `(QuestionVersionId, QuestionId)` → `QuestionVersion(Id, QuestionId)`; the same on `QuestionAttempt` |
| H3 | `Question.CurrentVersionId` could point at another question's version | Composite FK `(CurrentVersionId, Id)` |
| H4 | `ComponentId` and `CourseId` were stored side by side with nothing tying them (Slip, StudySession, PlanEntry, Question, Flashcard, AiConversation) | `UNIQUE (Id, CourseId)` on `CourseComponent`, and composite FKs everywhere. A slip can no longer name another course's lecture |
| H5 | Deleting a course failed whenever questions existed (cascade → component → RESTRICT on Question), and there was no soft delete for courses | `Course.ArchivedAt`; components and questions are soft-deleted; hard delete only through the deletion job |
| H6 | No concurrency control. Phone and laptop editing one weak spot or plan entry means the last write wins silently. The current board already needs `if_version` for this | `RowVersion INT` on every user-editable row (pattern in the file header) |
| H7 | No idempotency. A retried flashcard sync or a double-tap on submit duplicates reviews and attempts, which skews FSRS and stats | `ClientId` + `UNIQUE (UserId, ClientId)` on `FlashcardReview` and `QuestionAttempt`; `IdempotencyKey` on `AiRun`; `DedupeKey` on `Notification` |
| H8 | `PasswordHash NOT NULL`, and no session, reset or verify tables | `PasswordHash` nullable (SSO), plus `UserIdentity`, `UserSession` and `AuthToken`. Only token hashes are stored |
| H9 | Grades were `VARCHAR`, so there is no average and no pass/fail logic | `GradeValue DECIMAL` + `GradeText` + `GradeProvisional` (PES 9.3 "tentative until review"); `Deadline.ResitOfDeadlineId` keeps resit history linked |
| H10 | `DEFAULT CURRENT_TIMESTAMP` uses the **session** zone, so the "UTC" columns weren't UTC on a non-UTC server | Requirement in the header: `default_time_zone = '+00:00'` on the server and pool |
| H11 | Second-resolution timestamps. Two flashcard ratings in one second have no order, and FSRS replay needs that order | `DATETIME(3)` on answers, reviews, events and AI messages |
| H12 | Status and date pairs could disagree (`Done` without `FinishedAt`, `Resolved` without `ResolvedAt`, a grade without who graded it) | CHECKs on `ComponentProgress`, `WeakSpot`, `MockQuestion` and `FlashcardProgress` (suspended ⇔ reason) |

### Medium: completeness and parity with today's board

| # | Gap | v4 fix |
|---|---|---|
| M1 | Calculation questions (RK, CRS) could only be "Open", so they needed AI or self-grading | `NumericQuestion` (value, tolerance abs/rel, unit, unit required) graded automatically. Also cuts AI cost |
| M2 | Lecture notes with embedded check MCs had no link | `MaterialQuestion`; `QuestionAttempt.MaterialId` |
| M3 | "Untested", go-easy weights and miss replay needed a scan of every mock | `UserQuestionStat` read model (derived, rebuildable) |
| M4 | A slip-made flashcard didn't know its slip | `Flashcard.SlipId`; CHECK: a slip card must have an owner |
| M5 | Exam-eve sheet, check-in and "cards due" reminders had nowhere to live | `Notification` with a dedupe key |
| M6 | Files: no owner, no virus scan, cross-user dedupe (which leaks "someone else has this file") | `StoredFile.OwnerUserId`, `ScanStatus`, dedupe per owner only, `DeletedAt` + purge job |
| M7 | A mock couldn't be rebuilt or audited | `MockExam.Seed`, cached `Score`/`MaxScore` |
| M8 | `Event` is a MySQL keyword | Renamed `UserEvent` |
| M9 | Plan "Moved" lost where the entry went | `StudyPlanEntry.MovedToId` |

## 4. Scale: what breaks first, and the plan

Rough sizes per active student per term: about 2k flashcard reviews, 1k question answers, 10k events and 500 AI messages. MySQL handles this comfortably to roughly 10k–50k students on one primary with the indexes in v4. What to watch:

| Pressure | When | Plan already built in |
|---|---|---|
| `UserEvent` growth | First table past ~100M rows | No FKs and `PRIMARY KEY (Id, OccurredAt)`, so `PARTITION BY RANGE COLUMNS(OccurredAt)` monthly works without a migration (tested). Drop old partitions into cold storage |
| `FlashcardReview` growth | Next | Same pattern when needed (it has FKs today; drop them when partitioning) |
| Full-text search on `MaterialChunk` | Thousands of courses | `EmbeddingRef` already points to an external vector store; move keyword search to OpenSearch with the same chunk ids |
| Large text in MySQL | AI transcripts | Kept out: `AiRun.TranscriptRef` points to the object store with `RetainUntil` |
| Dashboard reads | Every page load | Build read models (`UserQuestionStat` style) from `UserEvent`; never compute week totals live |
| Changing requirements | Always | ENUMs append-only (instant in MySQL 8); `Extra JSON` / `BuildOptions JSON` / `GenerationPolicy JSON` for settings that are still moving; promote a JSON key to a column once it is queried |
| Multi-region / sharding | Far off | Every user-data table carries `UserId` and every catalog table carries `CourseId`, so sharding by user is possible without remodelling |

Operational baseline: migrations through a tool (Flyway or Liquibase, numbered files, no hand edits); daily backups plus binlog for point-in-time restore; a staging copy loaded from anonymised data.

## 5. AI safeguards (opt-in, everything works without it)

1. **Off means off.** No model call happens unless every one of these holds: `UserAiSettings.Enabled`, the feature's `Allow*` flag, `CourseSubscription.AiEnabled` is not FALSE (some courses forbid AI), `ComponentMaterial.AiAllowed` (licence or copyright) and `StoredFile.ScanStatus = 'Clean'`. The database refuses `Enabled = TRUE` without `ConsentedAt`.
2. **Provable consent.** `AiConsentLog` is append-only and records each change with its policy version.
3. **Budget that can't be raced.** `AiUsageMonthly` reserve-then-settle (SQL in the file). A failed reservation sets the run to `Blocked`.
4. **Transparency.** `AiRunContext` lists every record that went into the prompt, so the panel can show "the AI saw: your 3 open BSA weak spots, L4 slides p. 12–18".
5. **The AI never writes truth directly.**
   - Grades are `AiGrade` suggestions, which you accept or override (`DecidedBy`).
   - Import builds `ImportProposal` rows, reviewed before apply, with `UserEdits` kept to measure quality.
   - Generated questions get `ReviewState`, start private, and must never carry `KeyReliability = 'Official'`.
   - Slips the AI proposes are `LoggedBy = 'AI'` only after you accept them.
6. **Prompt injection.** Uploaded PDFs are untrusted. The app passes chunk text as data, never as instructions, and the import worker can only produce proposals, never direct writes. The schema supports this through the proposal staging. The rest is app code (spec).
7. **Secrets.** A bring-your-own API key is envelope-encrypted (`ApiKeyKeyId` for rotation). Only `ApiKeyLast4` is ever shown. The key is never logged.
8. **Retention.** Transcripts expire (`RetainUntil`). Account deletion removes runs, chats and events.

## 6. Rules MySQL can't enforce (each needs a service check + a unit test)

MySQL rejects a CHECK on any column that also has `ON DELETE SET NULL` (error 3823, verified). That is why some of these are app rules.

1. Slip and WeakSpotCheck: at most one source (mock / review / attempt).
2. Deadline: `ResitOfDeadlineId` only when `Type = 'Resit'`.
3. Question: `PaperQuestionNo` needs `PaperId`; Paper and Family in the same course; Concept in the question's course.
4. MCOption: at least 2 options and exactly 1 correct (multi-row rule).
5. Each `QuestionVersion` has exactly one key row of its type (presence; the type itself is enforced).
6. `QuestionVersion` rows are immutable: edits insert `VersionNo + 1`.
7. MockQuestion: the question is in `MockExam.CourseId` and visible to the mock's user.
8. ComponentDependency: no cycles, and the same course only.
9. AiConversation: the anchor (material, question, mock) belongs to `CourseId`.
10. Generated questions: never `Official`.
11. `UserEvent` dedupe on `(UserId, ClientId)` at ingest (it can't be UNIQUE once partitioned).
12. Access check from section 2 on every repository call.

## 7. Current board → v4 (nothing lost)

| Board today | v4 |
|---|---|
| `dashboard/state` courses, lectures, exams, weeks | `Course`, `CourseComponent`, `CourseSubscription`, `Deadline`, `ComponentProgress`, `StudySession`, `StudyPlanEntry` (weeks = query) |
| `questions/*` incl. family, key, source.paper | `Question` + `QuestionVersion` + key tables, `Paper`, `FamilyId` |
| `config/papers` (exam map) | `Paper`, `PaperOmission` |
| `mocks/*` (answers, conf, revealed, selfShown, open grades, fixedInReview) | `MockExam`, `MockQuestion`, `AiGrade` |
| `config/mockfmt` real exam layout | `DeadlineComponent` + `MockExam.DeadlineId` |
| `cards/*`, `log/*`, `flashsessions/*`, warm-up flag, codes, archive suspension | `Flashcard`, `FlashcardProgress`, `FlashcardReview`, `StudySession(Kind='Flashcards')`, `WarmupFlaggedAt`, `Code`, `SuspendReason` |
| `config/flash`, go-easy, misses-only | `UserSettings`, `MockExam.BuildOptions` |
| `config/topup` | `CourseSubscription.GenerationPolicy` |
| `config/traps` | `Trap` |
| `notesprog/*`, `noteslips/*` | `QuestionAttempt(Context='Notes')`, `Slip` |
| `inbox/*` (new course / add materials) | `CourseImport` (AI) or plain material upload (no AI) |
| OPEN weak-spot lines, cold passes | `WeakSpot`, `WeakSpotCheck` |
| Archived section with all analytics | `CourseSubscription.Status` + everything kept (`RESTRICT` on Course) |

## 8. Test results (MySQL 8.0.46)

Schema loads clean with 54 tables. Seed `schema-v4-smoke-test.sql` loads. Each of these was **rejected** as intended:

- An Open key on an MC version.
- A current version belonging to another question.
- A mock row showing another question's version, and an MC option from another version.
- A slip naming another course's lecture.
- A duplicate card code in one course (the same code in another user's course is allowed).
- A slip card without an owner.
- AI enabled without consent.
- Done without a finish date.
- A 3-day study session.
- A grade without a grader.
- Deleting a user who still owns a course.

These were **allowed** as intended:

- Deleting a mock keeps its slip and nulls the source.
- Deleting a subscription removes the exam and its linked resit.
- Monthly partitioning of `UserEvent`.

## 9. Not in v4 yet, and when to add

| Item | Add when |
|---|---|
| Multi-select MC (answer table instead of `ChosenOptionId`) | A course's exam uses it |
| Code questions with test cases (PES CodeGrade style) | A programming course is imported |
| FSRS on questions (not just cards) | After a term of `UserQuestionStat` data |
| Question quality stats (distractor pick rates across users) | Once public courses have more than ~20 subscribers |
| Organisations / teachers / classes | A lecturer wants to publish a course |
| Billing / plans | AI is offered on the platform's key beyond a free cap |
| GDPR export endpoint | Before public launch (the data is all user-keyed already) |
| iCal feed of deadlines and plan | Nice to have; `Deadline` + `StudyPlanEntry` already hold it |

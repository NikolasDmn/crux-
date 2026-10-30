# Plans, AI credits and billing

Status: agreed direction, not built. AI cost figures are estimates from a one-off model test (October 2026); the credit ledger (stage 2) replaces them with measured numbers.

## Decisions so far

- **One AI provider: Claude.** OpenAI and Gemini cost the same at the cheap end and did no better on the test. The ledger stays provider-neutral so a feature can switch later.
- **Hosted service only.** Users don't bring their own API key. The existing per-user key code stays behind a flag, off.
- **One paid plan, "Plus", bought three ways.** Same perks for all three; they differ only in length and AI allowance. Top-ups are AI credit only.

| | Free | Plus monthly | Semester pass | Exam pass | Top-up |
|---|---|---|---|---|---|
| Price (incl. VAT) | €0 | €3.99/mo, cancel anytime | €14.99 once, ~5 months | €6.99 once, 6 weeks | €2.99 |
| AI allowance (our cost) | €0.15/mo | €1.50/mo | €1.50/mo | €2.25 total | €1.00, lasts 12 months |
| Models | Quick, Standard (Haiku) | all | all | all | as the plan |
| Active owned courses | 3 | unlimited (hidden fair-use cap ~30) | same | same | — |
| Storage | 1 GiB | 10 GiB | 10 GiB | 10 GiB | — |
| Per week | — | €0.92 | €0.75 | €1.17 | — |

The ordering by value is semester pass, then monthly, then exam pass, then top-ups, on purpose.

## Smartness levels

Each AI feature has a level. The server maps it to a model and effort, and the user can raise it per feature in Settings.

| Level | Model | Relative cost | Free users |
|---|---|---|---|
| Quick | Haiku, low effort | 1× | yes |
| Standard | Haiku, high effort | ~2× | yes |
| Smart | Sonnet | ~20× | no |
| Genius | Opus | ~40× | no |

Default level per feature:
- Ask and explanations: Quick.
- Grading, question writing and imports: Standard.

Every Derived answer key gets a **second opinion**: it's worked out twice, and when the two answers disagree, the key is flagged "unconfirmed" instead of being shown as certain. In the test, Haiku made one wrong key in 16, so this is needed whichever model is used.

## Stage 1: model routing

- A server-side table maps (feature, level) to model, effort and max tokens. It lives in config, not code, so prices or models can change without a release.
- Users get a per-feature level preference, stored in settings `extra`. The plan decides which levels a user may pick.
- Add the second-opinion check for Derived keys in generation and imports.
- Done when every AI call goes through the router and the tests cover tier gating.

## Stage 2: credit ledger

- An `AiUsage` row per call: user, feature, level, model, input, output and cache tokens, and cost in micro-euros.
- Balances in buckets, spent oldest-expiring first:
  - the plan's monthly allowance, which resets each period;
  - the exam pass allowance;
  - top-ups, each expiring 12 months after purchase.
- Before a call:
  - estimate its cost and refuse if the balance can't cover it;
  - imports show the estimate and ask first;
  - a per-user daily cap stops runaway loops.
- **Global free budget**, about $15 a month to start:
  - alert at 80%;
  - at 100%, free AI pauses until the 1st with a friendly message. Paid users are unaffected.
- Spend limit on the Anthropic account as the last backstop.
- What users see:
  - Free users see credits (1 credit = €0.001 of cost, so 150 a month).
  - Paid users see "fair use" and get a warning at 80%.
  - A usage page shows plain equivalents ("≈ 150 questions answered").
- The current spend cap in the user's currency becomes the usage page.

## Stage 3: plans and limits

- Plans already exist in the database, each with a storage quota and a course limit. Add entitlements: plan, source (monthly, semester, exam pass, admin grant), start, end and the AI allowance.
- Course limit counts **active owned** courses. Shared and archived courses don't count.
- Archived courses on free:
  - **Readable forever:** notes, questions and past papers.
  - **Not** practised, planned, edited or used with AI.
  - Unarchiving is limited to **2 per month**.
  - On Plus there are no limits.
- Downgrade (a pass ends, or a subscription is cancelled):
  - nothing is deleted;
  - the user picks which 3 courses stay active, and the rest are archived;
  - over the storage limit, existing files stay but new uploads are blocked until they're back under it;
  - AI drops to the free allowance at the next reset, and top-ups keep working.
- Upgrade prompts appear at the moment of friction, e.g. the 4th course or an empty balance, with the cheapest fitting option first.
- Admin can grant Plus by hand, for testing and before payments go live.
- Measure: limit hits, what follows (archive, purchase or leaving), AI cost per user per plan.

## Stage 4: payments

- Choose a payment provider (see the legal notes): a merchant of record (Paddle or Lemon Squeezy) or Stripe plus Stripe Tax.
- Checkout for monthly, semester, exam pass and top-ups. Webhooks create and end entitlements, and the provider's customer portal handles cancelling.
- At checkout:
  - the EU withdrawal waiver as an explicit tick box ("I want access now and lose my 14-day withdrawal right");
  - prices shown including VAT.
- Invoices or receipts, refunds and chargebacks are handled through the provider.

## Stage 5: launch checklist

Legal and admin items are covered in the notes below. Engineering items:
- Label AI output wherever it appears: answers, generated questions and grades (EU AI Act, Article 50).
- Age gate at sign-up.
- Terms and privacy pages linked from sign-up and checkout.
- A notice-and-takedown contact for uploaded material.

## Legal and admin notes (Netherlands)

Not legal advice. Check with KVK's free starter advice or an accountant before taking money.

- **Register a business:** an eenmanszaak (sole trader) at KVK, a one-off fee of about €85 in 2026 and no yearly fee. You're personally liable, so consider a BV once revenue is real.
- **VAT:**
  - Dutch VAT applies, and EU customers' own VAT once cross-border sales to consumers pass €10,000 a year, filed through OSS.
  - The KOR small-business scheme exempts Dutch turnover up to €20,000.
  - A merchant of record removes nearly all of this. They sell to the customer and you invoice them. It costs a higher fee, about 5% plus €0.50 a sale, which matters on €3.99.
- **Income tax:**
  - Profit goes in box 1.
  - Startersaftrek needs 1,225 hours a year, unlikely while studying.
  - Profit can affect toeslagen, e.g. zorgtoeslag.
  - Keep records for 7 years, and use a separate bank account (sensible, not required).
- **Privacy (AVG/GDPR):**
  - a privacy policy;
  - processor agreements with Anthropic, the hosting provider and the payment provider;
  - a record of processing;
  - reporting data breaches to the Autoriteit Persoonsgegevens within 72 hours.
  - Course files and answers are personal data.
- **Age:** sign-up should require 16+, the Dutch age of digital consent, or 18+, which also keeps clear of AI-provider rules for minors (check Anthropic's usage policy).
- **Consumer law:**
  - terms (algemene voorwaarden) given before purchase;
  - the 14-day withdrawal waiver for instant digital access;
  - clear cancellation for subscriptions.
- **EU AI Act, Article 50 (applies since 2 August 2026):** users must be told they're dealing with AI, prominently, not buried in the terms.
- **Uploaded course material:** the terms say users upload only material they may use for personal study. Provide a notice-and-takedown contact, since the EU Digital Services Act applies to hosting services.
- **Cookies:** only functional cookies today, so no consent banner is needed. Keep it that way, or add one with analytics.

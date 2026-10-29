import { createFileRoute } from "@tanstack/react-router";
import { useEffect, useState, type FormEvent } from "react";
import { useUser } from "@/app/auth";
import { Badge } from "@/components/ui/badge";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Card, SectionLabel } from "@/components/ui/card";
import { Check, Field, Input, Select, Textarea } from "@/components/ui/field";
import { Skeleton } from "@/components/ui/misc";
import { Dialog } from "@/components/ui/overlays";
import { useToast } from "@/components/ui/toast";
import {
  useAddKey,
  useAiSettings,
  useKeys,
  usePatchAiSettings,
  useRevokeKey,
  useSaveTutor,
  useTutors,
  useUsage,
  useVerifyKey,
  type AiSettings,
  type AiSettingsPatch,
  type Tutor,
} from "@/features/ai/api";
import {
  FEATURE_LABEL,
  MAX_TUTOR_TEXT,
  PERSONALITIES,
  priceLine,
  PROVIDER_LABEL,
  readRules,
  STYLE_LABEL,
  STYLES,
} from "@/features/ai/lib/format";
import { usePatchSettings } from "@/features/account/api";
import { CURRENCIES, capInput, capText, currencyFor, isCurrency, money, type Currency } from "@/features/ai/lib/money";
import { useSettings } from "@/features/plan/api";
import type { Schemas } from "@/lib/api";
import { fieldErrors, messageFor } from "@/lib/errors";
import { formatInZone } from "@/lib/time";

// AI (frontend-spec 4): the master switch with the consent text and policy version, per-feature
// switches, data sharing, API keys (only the last 4 digits shown), the default model with its price,
// tutors, the monthly cap and usage. AI is opt-in (2.5): with it off, only the switch shows.
export const Route = createFileRoute("/_app/settings/ai")({ component: Ai });

function usePatch() {
  const patch = usePatchAiSettings();
  const toast = useToast();
  return (body: AiSettingsPatch, ok = "Saved") =>
    patch.mutate(body, {
      onSuccess: () => toast({ message: ok }),
      onError: (e) => toast({ message: messageFor(e), tone: "danger" }),
    });
}

function Consent({ s }: { s: AiSettings }) {
  const user = useUser();
  const patch = usePatch();
  const outdated = s.enabled && s.consentPolicyVersion !== s.currentPolicyVersion;
  return (
    <Card className="flex flex-col gap-3">
      <SectionLabel>AI features</SectionLabel>
      {s.enabled ? (
        <>
          <p className="m-0">
            AI is on. You agreed to the AI policy {s.consentPolicyVersion}
            {s.consentedAt ? ` on ${formatInZone(s.consentedAt, user.timeZone, "d MMM yyyy")}` : ""}.
          </p>
          {outdated ? (
            <Banner tone="warning">
              <span className="flex flex-wrap items-center gap-3">
                <span className="flex-1">The AI policy changed to {s.currentPolicyVersion}. Read it below again.</span>
                <Button
                  size="sm"
                  onClick={() => patch({ consentPolicyVersion: s.currentPolicyVersion }, "Thanks, you're up to date")}
                >
                  I agree
                </Button>
              </span>
            </Banner>
          ) : null}
        </>
      ) : null}
      {!s.enabled || outdated ? <PolicyText version={s.currentPolicyVersion} /> : null}
      <div>
        {s.enabled ? (
          <Button onClick={() => patch({ enabled: false }, "AI is off")}>Turn off AI</Button>
        ) : (
          <Button
            variant="primary"
            onClick={() => patch({ enabled: true, consentPolicyVersion: s.currentPolicyVersion }, "AI is on")}
          >
            I agree, turn on AI
          </Button>
        )}
      </div>
    </Card>
  );
}

function PolicyText({ version }: { version: string }) {
  return (
    <div className="flex flex-col gap-2 rounded-md bg-sunken p-4 text-sm">
      <p className="m-0 font-medium">AI policy {version}</p>
      <ul className="m-0 flex list-disc flex-col gap-1 pl-5">
        <li>
          AI runs on your own API key with a provider you pick (Anthropic, OpenAI, Google or Mistral). The provider
          bills you; Crux shows estimated costs and can stop at a monthly cap.
        </li>
        <li>
          When you use an AI feature, the parts of your course it needs (material text, questions, your answers) go to
          that provider under its own terms. Materials blocked from AI never go.
        </li>
        <li>
          Your progress, mistakes and answers are sent only if you allow each of them below. You can change that, or
          turn AI off, at any time.
        </li>
        <li>Crux keeps a record of each AI request for 90 days, then deletes it. Your key is stored encrypted.</li>
        <li>AI can be wrong. Generated questions are marked, and grades it suggests wait for you to accept them.</li>
      </ul>
    </div>
  );
}

function Features({ s }: { s: AiSettings }) {
  const patch = usePatch();
  const row = (key: keyof AiSettingsPatch, label: string, hint?: string) => (
    <Check
      key={key}
      checked={Boolean(s[key as keyof AiSettings])}
      onChange={(v) => patch({ [key]: v })}
      label={label}
      hint={hint}
    />
  );
  return (
    <Card className="flex flex-col gap-4">
      <SectionLabel>What AI may do</SectionLabel>
      {row("allowMaterialChat", FEATURE_LABEL.MaterialChat, "A side panel to ask about a material, question or mock.")}
      {row("allowGrading", FEATURE_LABEL.Grading, "Suggests a grade and feedback; you accept or change it.")}
      {row("allowCourseImport", FEATURE_LABEL.CourseImport, "Reads your PDFs and proposes lectures, papers and dates.")}
      {row("allowPastExamImport", FEATURE_LABEL.PastExamImport)}
      {row("allowGeneration", FEATURE_LABEL.Generation)}
      {row("nightlyGeneration", "Top up practice questions every night", "Before your exams, at 03:00 your time.")}
      {row(
        "autoApproveGenerated",
        "Approve generated questions automatically",
        "Off: they wait as drafts in the question bank until you approve them.",
      )}
    </Card>
  );
}

function Sharing({ s }: { s: AiSettings }) {
  const patch = usePatch();
  return (
    <Card className="flex flex-col gap-4">
      <SectionLabel>What AI may see about you</SectionLabel>
      <Check
        checked={s.shareProgress}
        onChange={(v) => patch({ shareProgress: v })}
        label="Progress"
        hint="Which lectures you've done and today's plan, so answers fit where you are."
      />
      <Check
        checked={s.shareMistakes}
        onChange={(v) => patch({ shareMistakes: v })}
        label="Mistakes"
        hint="Your weak spots, recent slips and traps, so it can point at what you tend to get wrong."
      />
      <Check
        checked={s.shareAnswers}
        onChange={(v) => patch({ shareAnswers: v })}
        label="Answers"
        hint="Your answer to the question you're asking about. Hidden answer keys are never sent."
      />
    </Card>
  );
}

/** `providers` comes from the server: the dev fake shows only where the ai service allows it. */
function AddKey({ providers }: { providers: string[] }) {
  const add = useAddKey();
  const toast = useToast();
  const [provider, setProvider] = useState<string>("anthropic");
  const [label, setLabel] = useState("default");
  const [secret, setSecret] = useState("");
  const errors = fieldErrors(add.error);
  const submit = (e: FormEvent) => {
    e.preventDefault();
    add.mutate(
      { provider, label: label.trim() || "default", apiKey: secret.trim() },
      {
        onSuccess: (k) => {
          setSecret("");
          toast({
            message:
              k.status === "Valid"
                ? "Key added and checked"
                : k.status === "Invalid"
                  ? "Key added, but the provider rejected it"
                  : "Key added; the provider couldn't be reached to check it",
            tone: k.status === "Invalid" ? "danger" : undefined,
          });
        },
      },
    );
  };
  return (
    <form className="grid gap-3 sm:grid-cols-[10rem_10rem_1fr_auto] sm:items-end" onSubmit={submit} noValidate>
      <Field label="Provider" error={errors.provider}>
        {(ids) => (
          <Select {...ids} value={provider} onChange={(e) => setProvider(e.currentTarget.value)}>
            {providers.map((p) => (
              <option key={p} value={p}>
                {PROVIDER_LABEL[p] ?? p}
              </option>
            ))}
          </Select>
        )}
      </Field>
      <Field label="Label" error={errors.label}>
        {(ids) => <Input {...ids} value={label} maxLength={64} onChange={(e) => setLabel(e.currentTarget.value)} />}
      </Field>
      <Field label="API key" error={errors.apiKey}>
        {(ids) => (
          <Input
            {...ids}
            type="password"
            autoComplete="off"
            spellCheck={false}
            value={secret}
            onChange={(e) => setSecret(e.currentTarget.value)}
          />
        )}
      </Field>
      <Button type="submit" variant="primary" disabled={add.isPending || secret.trim().length < 8}>
        {add.isPending ? "Checking…" : "Add key"}
      </Button>
      {add.error && !Object.keys(errors).length ? (
        <p className="m-0 text-sm text-danger-text sm:col-span-4">{messageFor(add.error)}</p>
      ) : null}
    </form>
  );
}

function Keys({ s }: { s: AiSettings }) {
  const user = useUser();
  const keys = useKeys();
  const verify = useVerifyKey();
  const revoke = useRevokeKey();
  const patch = usePatch();
  const toast = useToast();
  const [revoking, setRevoking] = useState<string | null>(null);
  const onError = (e: unknown) => toast({ message: messageFor(e), tone: "danger" });
  const list = keys.data ?? [];
  const def = list.find((k) => k.isDefault);
  return (
    <Card className="flex flex-col gap-4">
      <SectionLabel>API keys</SectionLabel>
      {keys.isPending ? (
        <Skeleton className="h-16" />
      ) : list.length ? (
        <ul className="m-0 flex list-none flex-col divide-y divide-border p-0">
          {list.map((k) => (
            <li key={k.id} className="flex flex-wrap items-center gap-x-3 gap-y-2 py-2.5">
              <span className="flex min-w-0 flex-1 flex-col">
                <span>
                  {PROVIDER_LABEL[k.provider] ?? k.provider} · {k.label}{" "}
                  <span className="font-mono text-sm text-muted">••••{k.last4}</span>
                </span>
                <span className="text-sm text-muted">
                  {k.lastVerifiedAt
                    ? `Checked ${formatInZone(k.lastVerifiedAt, user.timeZone, "d MMM, HH:mm")}`
                    : "Not checked yet"}
                </span>
              </span>
              {k.isDefault ? <Badge kind="label">Default</Badge> : null}
              <Badge tone={k.status === "Valid" ? "success" : k.status === "Invalid" ? "danger" : "neutral"}>
                {k.status}
              </Badge>
              <span className="flex gap-1">
                {!k.isDefault && k.status !== "Invalid" ? (
                  <Button size="sm" variant="ghost" onClick={() => patch({ defaultKeyId: k.id })}>
                    Make default
                  </Button>
                ) : null}
                <Button
                  size="sm"
                  variant="ghost"
                  disabled={verify.isPending}
                  onClick={() => verify.mutate(k.id, { onError })}
                >
                  Check
                </Button>
                <Button size="sm" variant="ghost" className="text-danger-text" onClick={() => setRevoking(k.id)}>
                  Revoke
                </Button>
              </span>
            </li>
          ))}
        </ul>
      ) : (
        <p className="m-0 text-muted">No keys yet. AI needs one from a provider you have an account with.</p>
      )}
      <AddKey providers={s.providers} />
      {def ? (
        <Field
          label="Model"
          hint={(() => {
            const p = def.prices.find((x) => x.id === s.defaultModel);
            return p ? `${priceLine(p)} (estimate; your provider bills you)` : undefined;
          })()}
        >
          {(ids) => (
            <Select
              {...ids}
              className="sm:max-w-80"
              value={s.defaultModel ?? ""}
              onChange={(e) => patch({ defaultModel: e.currentTarget.value })}
            >
              {s.defaultModel ? null : <option value="">Pick a model</option>}
              {def.prices.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.id}
                  {p.default ? " (recommended)" : ""}
                </option>
              ))}
            </Select>
          )}
        </Field>
      ) : null}
      <Dialog
        open={revoking !== null}
        onOpenChange={(o) => !o && setRevoking(null)}
        title="Revoke this key?"
        description="Crux deletes it and stops using it. Also revoke it at your provider if it may have leaked."
        footer={
          <>
            <Button onClick={() => setRevoking(null)}>Cancel</Button>
            <Button
              variant="destructive"
              disabled={revoke.isPending}
              onClick={() => revoke.mutate(revoking!, { onSuccess: () => setRevoking(null), onError })}
            >
              Revoke
            </Button>
          </>
        }
      />
    </Card>
  );
}

function TutorDialog({ tutor, onClose }: { tutor: Tutor | "new"; onClose: () => void }) {
  const save = useSaveTutor();
  const toast = useToast();
  const t = tutor === "new" ? null : tutor;
  const [name, setName] = useState(t?.name ?? "");
  const [description, setDescription] = useState(t?.description ?? "");
  const [personality, setPersonality] = useState<string>(t?.personality ?? "Warm");
  const [style, setStyle] = useState<string>(t?.teachingStyle ?? "ExplainFirst");
  const [rules, setRules] = useState(() => readRules(t?.rules));
  const [text, setText] = useState("");
  const errors = fieldErrors(save.error);
  const submit = () =>
    save.mutate(
      {
        ...(t ? { id: t.id } : {}),
        body: {
          name: name.trim(),
          description: description.trim(),
          personality,
          teachingStyle: style,
          rules: { ...rules },
          text,
        },
      },
      {
        onSuccess: () => {
          toast({ message: t ? "Tutor saved" : "Tutor created" });
          onClose();
        },
      },
    );
  return (
    <Dialog
      open
      onOpenChange={(o) => !o && onClose()}
      title={t ? `Edit ${t.name}` : "New tutor"}
      className="w-[min(560px,calc(100vw-32px))]"
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={save.isPending || !name.trim()} onClick={submit}>
            Save
          </Button>
        </>
      }
    >
      {save.error && !Object.keys(errors).length ? <Banner tone="danger">{messageFor(save.error)}</Banner> : null}
      <Field label="Name" error={errors.name}>
        {(ids) => <Input {...ids} value={name} maxLength={64} onChange={(e) => setName(e.currentTarget.value)} />}
      </Field>
      <Field label="Description" error={errors.description} hint="Optional">
        {(ids) => (
          <Input {...ids} value={description} maxLength={255} onChange={(e) => setDescription(e.currentTarget.value)} />
        )}
      </Field>
      <div className="grid gap-3 sm:grid-cols-2">
        <Field label="Personality" error={errors.personality}>
          {(ids) => (
            <Select {...ids} value={personality} onChange={(e) => setPersonality(e.currentTarget.value)}>
              {PERSONALITIES.map((p) => (
                <option key={p}>{p}</option>
              ))}
            </Select>
          )}
        </Field>
        <Field label="Teaching style" error={errors.teachingStyle}>
          {(ids) => (
            <Select {...ids} value={style} onChange={(e) => setStyle(e.currentTarget.value)}>
              {STYLES.map((p) => (
                <option key={p} value={p}>
                  {STYLE_LABEL[p]}
                </option>
              ))}
            </Select>
          )}
        </Field>
      </div>
      <Check
        checked={rules.approachCheckYesNoHint}
        onChange={(v) => setRules({ ...rules, approachCheckYesNoHint: v })}
        label={'"Is my approach right?" gets yes, or no plus one hint'}
      />
      <Check
        checked={rules.oneQuestionAtATime}
        onChange={(v) => setRules({ ...rules, oneQuestionAtATime: v })}
        label="One question at a time"
      />
      <div className="grid gap-3 sm:grid-cols-2">
        <Field label="Hints before the solution" error={errors["rules.hintsBeforeSolution"]} hint="0 to 5">
          {(ids) => (
            <Input
              {...ids}
              type="number"
              min={0}
              max={5}
              value={rules.hintsBeforeSolution}
              onChange={(e) => setRules({ ...rules, hintsBeforeSolution: Number(e.currentTarget.value) })}
            />
          )}
        </Field>
        <Field label="Longest reply (words)" error={errors["rules.maxWords"]} hint="50 to 1,000">
          {(ids) => (
            <Input
              {...ids}
              type="number"
              min={50}
              max={1000}
              step={50}
              value={rules.maxWords}
              onChange={(e) => setRules({ ...rules, maxWords: Number(e.currentTarget.value) })}
            />
          )}
        </Field>
      </div>
      <Field
        label="Anything else"
        error={errors.text}
        hint={`${text.length} / ${MAX_TUTOR_TEXT}${t ? ". Not shown again after saving; saving replaces what you wrote before." : ""}`}
      >
        {(ids) => (
          <Textarea
            {...ids}
            rows={4}
            maxLength={MAX_TUTOR_TEXT}
            value={text}
            placeholder="E.g. use examples from economics."
            onChange={(e) => setText(e.currentTarget.value)}
          />
        )}
      </Field>
    </Dialog>
  );
}

function Tutors({ s }: { s: AiSettings }) {
  const tutors = useTutors();
  const patch = usePatch();
  const [editing, setEditing] = useState<Tutor | "new" | null>(null);
  const list = tutors.data ?? [];
  return (
    <Card className="flex flex-col gap-4">
      <SectionLabel>Tutor</SectionLabel>
      <Field label="Default tutor" hint="A course or a conversation can pick another one.">
        {(ids) => (
          <Select
            {...ids}
            className="sm:max-w-80"
            // With none chosen, the AI service uses the built-in Friendly explainer.
            value={s.defaultTutorId ?? list.find((t) => t.key === "friendly-explainer")?.id ?? ""}
            onChange={(e) => patch({ defaultTutorId: e.currentTarget.value })}
          >
            {list.map((t) => (
              <option key={t.id} value={t.id}>
                {t.name}
                {t.builtIn ? " (built in)" : ""}
              </option>
            ))}
          </Select>
        )}
      </Field>
      <ul className="m-0 flex list-none flex-col divide-y divide-border p-0">
        {list.map((t) => (
          <li key={t.id} className="flex items-center gap-3 py-2.5">
            <span className="flex min-w-0 flex-1 flex-col">
              <span>{t.name}</span>
              <span className="text-sm text-muted">
                {t.description || `${t.personality}, ${STYLE_LABEL[t.teachingStyle] ?? t.teachingStyle}`}
              </span>
            </span>
            {t.builtIn ? (
              <Badge kind="label">Built in</Badge>
            ) : (
              <Button size="sm" variant="ghost" onClick={() => setEditing(t)}>
                Edit
              </Button>
            )}
          </li>
        ))}
      </ul>
      <div>
        <Button onClick={() => setEditing("new")}>New tutor</Button>
      </div>
      {editing ? <TutorDialog tutor={editing} onClose={() => setEditing(null)} /> : null}
    </Card>
  );
}

/** Your currency for AI money: chosen here (settings.extra.currency), else your region's. */
function useCurrency(): [Currency, (c: Currency) => void] {
  const settings = useSettings();
  const save = usePatchSettings();
  const extra = (settings.data?.extra ?? {}) as Record<string, unknown>;
  const cur = isCurrency(extra.currency) ? extra.currency : currencyFor(navigator.language);
  const set = (c: Currency) => {
    if (!settings.data) return;
    save.mutate({
      version: settings.data.version,
      extra: { ...extra, currency: c } as unknown as Schemas["SettingsPatch"]["extra"],
    });
  };
  return [cur, set];
}

function Spending({ s }: { s: AiSettings }) {
  const usage = useUsage();
  const patch = usePatch();
  const [cur, setCur] = useCurrency();
  const [cap, setCap] = useState(() => capText(s.monthlyCapMicroUsd, cur));
  // Show the cap in the currency once it's known (settings load after the first render).
  useEffect(() => setCap(capText(s.monthlyCapMicroUsd, cur)), [cur, s.monthlyCapMicroUsd]);
  const parsed = capInput(cap, cur);
  const current = s.monthlyCapMicroUsd ?? "none";
  return (
    <Card className="flex flex-col gap-4">
      <SectionLabel>Spending</SectionLabel>
      <form
        className="flex flex-wrap items-end gap-3"
        onSubmit={(e) => {
          e.preventDefault();
          if (parsed !== null) patch({ monthlyCapMicroUsd: parsed === "none" ? 0 : parsed });
        }}
      >
        <Field label="Currency" className="w-28">
          {(ids) => (
            <Select {...ids} value={cur} onChange={(e) => setCur(e.currentTarget.value as Currency)}>
              {Object.keys(CURRENCIES).map((c) => (
                <option key={c}>{c}</option>
              ))}
            </Select>
          )}
        </Field>
        <Field
          label={`Monthly cap (${cur})`}
          error={parsed === null ? "Use an amount like 5 or 2.50." : undefined}
          hint={
            cur === "USD"
              ? "AI stops for the month once its estimated cost reaches this. Empty: no cap."
              : "AI stops for the month once its estimated cost reaches this. Providers bill in USD, so other currencies are approximate. Empty: no cap."
          }
          className="sm:max-w-96"
        >
          {(ids) => <Input {...ids} inputMode="decimal" value={cap} onChange={(e) => setCap(e.currentTarget.value)} />}
        </Field>
        <Button type="submit" disabled={parsed === null || parsed === current}>
          Save
        </Button>
      </form>
      {usage.data?.length ? (
        <div className="-mx-1 overflow-x-auto px-1">
          <table className="w-full border-collapse text-left">
            <thead>
              <tr className="text-sm text-muted">
                <th className="py-2 pr-4 font-medium">Month</th>
                <th className="py-2 pr-4 text-right font-medium">Requests</th>
                <th className="py-2 pr-4 text-right font-medium">Estimated cost</th>
                <th className="py-2 font-medium">By feature</th>
              </tr>
            </thead>
            <tbody>
              {usage.data.map((m) => (
                <tr key={m.month} className="border-t border-border align-top">
                  <td className="py-2.5 pr-4 font-mono tnum">{m.month}</td>
                  <td className="py-2.5 pr-4 text-right font-mono tnum">{m.runs}</td>
                  <td className="py-2.5 pr-4 text-right font-mono tnum">
                    {money(m.spentMicroUsd, cur)}
                    {m.capMicroUsd ? <span className="text-muted"> / {money(m.capMicroUsd, cur)}</span> : null}
                  </td>
                  <td className="py-2.5 text-sm text-muted">
                    {Object.entries(m.byFeature)
                      .map(([f, c]) => `${FEATURE_LABEL[f as keyof typeof FEATURE_LABEL] ?? f} ${money(c, cur)}`)
                      .join(" · ") || "–"}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <p className="m-0 text-sm text-muted">No AI use yet.</p>
      )}
    </Card>
  );
}

function Ai() {
  const settings = useAiSettings();
  if (settings.isError)
    return (
      <Banner tone="notice">
        AI settings can't be reached right now. Everything else in Crux keeps working; try again later.
      </Banner>
    );
  if (!settings.data) return <Skeleton className="h-96" />;
  const s = settings.data;
  return (
    <div className="flex max-w-[760px] flex-col gap-4">
      <Consent s={s} />
      {s.enabled ? (
        <>
          <Keys s={s} />
          <Features s={s} />
          <Sharing s={s} />
          <Tutors s={s} />
          <Spending s={s} />
        </>
      ) : null}
    </div>
  );
}

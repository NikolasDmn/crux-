import { useQueryClient } from "@tanstack/react-query";
import { createFileRoute, Link, useBlocker, useNavigate } from "@tanstack/react-router";
import { Flag, Grid3x3, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { Markdown } from "@/components/markdown";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Card, SectionLabel } from "@/components/ui/card";
import { Input, Textarea } from "@/components/ui/field";
import { Kbd, Skeleton } from "@/components/ui/misc";
import { OptionRow } from "@/components/ui/option-row";
import { Dialog } from "@/components/ui/overlays";
import { Segmented } from "@/components/ui/segmented";
import { useToast } from "@/components/ui/toast";
import { useCourse } from "@/features/courses/api";
import { useTraps } from "@/features/mistakes/api";
import {
  keys,
  saveAnswer,
  startMock,
  submitMock,
  useMock,
  type AnswerInput,
  type Item,
  type Mock,
} from "@/features/mocks/api";
import { crossed, fromItem, loadPending, navState, remaining, savePending } from "@/features/mocks/lib/taking";
import { cn } from "@/lib/cn";
import { messageFor } from "@/lib/errors";
import { mockKey, typingIn, type Confidence } from "@/lib/keyboard";
import { clock } from "@/lib/time";

// Taking a mock (frontend-spec 4, 5.3), full screen: the start screen with your traps and the
// rules; then one question per screen with confidence, flag, the navigator and the countdown.
// Every change saves (500 ms debounce); unsaved answers wait in sessionStorage and retry. At 0 it
// submits on its own and keeps retrying. Leaving asks first.
export const Route = createFileRoute("/_app/mock/$mockId/")({
  staticData: { focus: true },
  component: Taking,
});

function Taking() {
  const { mockId } = Route.useParams();
  const navigate = useNavigate();
  const data = useMock(mockId);
  const status = data.data?.mock.status;

  useEffect(() => {
    if (status && status !== "Draft" && status !== "InProgress") {
      void navigate({ to: "/mock/$mockId/result", params: { mockId }, replace: true });
    }
  }, [status, mockId, navigate]);

  if (data.isPending) return <Skeleton className="m-6 h-96" />;
  if (data.isError) return <Banner tone="danger">{messageFor(data.error)}</Banner>;
  const { mock, offset } = data.data;
  if (mock.status === "Draft") return <Start mock={mock} />;
  if (mock.status === "InProgress") return <Runner mock={mock} offset={offset} />;
  return null;
}

function Shell({ mock, children, right }: { mock: Mock; children: React.ReactNode; right?: React.ReactNode }) {
  const course = useCourse(mock.courseId);
  const style = { "--course": course.data?.color ?? "var(--text-muted)" } as CSSProperties;
  return (
    <div className="flex min-h-screen flex-col" style={style}>
      <header className="sticky top-0 z-20 flex h-14 items-center gap-3 border-b border-border bg-bg px-4">
        <Button asChild size="iconSm" variant="ghost" aria-label="Close">
          <Link to="/c/$courseId/mocks" params={{ courseId: mock.courseId }}>
            <X className="size-4" />
          </Link>
        </Button>
        <span className="font-mono text-sm font-medium">{course.data?.code}</span>
        <span className="min-w-0 truncate text-sm text-muted">{mock.name}</span>
        <span className="flex-1" />
        {right}
      </header>
      {children}
    </div>
  );
}

function Start({ mock }: { mock: Mock }) {
  const qc = useQueryClient();
  const traps = useTraps(mock.courseId);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const start = async () => {
    setBusy(true);
    try {
      qc.setQueryData(keys.mock(mock.id), await startMock(mock.id));
    } catch (e) {
      setError(messageFor(e));
      setBusy(false);
    }
  };
  return (
    <Shell mock={mock}>
      <div className="mx-auto flex w-full max-w-[640px] flex-col gap-5 px-4 py-8">
        <h1 className="m-0 text-xl font-semibold">{mock.name}</h1>
        {mock.warnings.map((w, i) => (
          <Banner key={i} tone="warning">
            The bank had {w.got} of the {w.wanted} questions wanted{w.componentId ? " for one lecture" : ""}; the mock
            is shorter.
          </Banner>
        ))}
        {traps.data?.length ? (
          <Card className="flex flex-col gap-2">
            <SectionLabel>Your traps</SectionLabel>
            <ul className="m-0 flex flex-col gap-1.5 pl-5">
              {traps.data.map((t) => (
                <li key={t.id}>{t.text}</li>
              ))}
            </ul>
          </Card>
        ) : null}
        <Card className="flex flex-col gap-2 text-sm">
          <SectionLabel>How it works</SectionLabel>
          <p className="m-0">
            {mock.questionCount} questions{mock.timeLimitMin ? `, ${mock.timeLimitMin} minutes` : ", no time limit"}.
            Answers save as you go; if the time runs out, the mock hands itself in.
          </p>
          <p className="m-0">Say how sure you are: a lucky guess counts as a miss, so the review shows what to fix.</p>
          <p className="m-0 text-muted">
            Keys: <Kbd>1</Kbd>–<Kbd>9</Kbd> choose (sure), <Kbd>Shift</Kbd> unsure, <Kbd>Alt</Kbd> guess,{" "}
            <Kbd>Space</Kbd> next, <Kbd>F</Kbd> flag.
          </p>
        </Card>
        {error ? <Banner tone="danger">{error}</Banner> : null}
        <Button variant="primary" size="lg" disabled={busy} onClick={() => void start()}>
          Start
        </Button>
      </div>
    </Shell>
  );
}

type Save = "saved" | "saving" | "waiting";

function Runner({ mock, offset }: { mock: Mock; offset: number }) {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const toast = useToast();
  const items = useMemo(() => [...mock.items].sort((a, b) => a.order - b.order), [mock.items]);
  const [i, setI] = useState(0);
  const [answers, setAnswers] = useState<Record<number, AnswerInput>>(() => {
    const saved = Object.fromEntries(items.map((it) => [it.order, fromItem(it)]));
    return { ...saved, ...loadPending(mock.id) };
  });
  const pending = useRef<Record<number, AnswerInput>>(loadPending(mock.id));
  const timers = useRef<Record<number, ReturnType<typeof setTimeout>>>({});
  const [save, setSave] = useState<Save>(Object.keys(pending.current).length ? "waiting" : "saved");
  const [left, setLeft] = useState(() => remaining(mock.endsAt, offset));
  const [nav, setNav] = useState(false);
  const [confirm, setConfirm] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const done = useRef(false);
  const item = items[i]!;
  const a = answers[item.order];

  const flush = useCallback(
    async (order: number) => {
      const body = pending.current[order];
      if (!body) return true;
      try {
        await saveAnswer(mock.id, order, body);
        if (pending.current[order] === body) delete pending.current[order];
        savePending(mock.id, pending.current);
        return true;
      } catch {
        return false;
      }
    },
    [mock.id],
  );

  const flushAll = useCallback(async () => {
    const orders = Object.keys(pending.current).map(Number);
    const ok = (await Promise.all(orders.map(flush))).every(Boolean);
    setSave(Object.keys(pending.current).length ? "waiting" : "saved");
    return ok;
  }, [flush]);

  // Retry what's waiting: every 5 s and when the connection comes back.
  useEffect(() => {
    const retry = () => void (Object.keys(pending.current).length && flushAll());
    const t = setInterval(retry, 5000);
    window.addEventListener("online", retry);
    retry();
    return () => {
      clearInterval(t);
      window.removeEventListener("online", retry);
    };
  }, [flushAll]);

  const change = (order: number, patch: Partial<AnswerInput>) => {
    setAnswers((all) => {
      const next = { flagged: false, ...all[order], ...patch };
      pending.current[order] = next;
      savePending(mock.id, pending.current);
      return { ...all, [order]: next };
    });
    setSave("saving");
    clearTimeout(timers.current[order]);
    timers.current[order] = setTimeout(() => {
      void flush(order).then(() => setSave(Object.keys(pending.current).length ? "waiting" : "saved"));
    }, 500);
  };

  const submit = useCallback(async () => {
    if (done.current) return;
    done.current = true;
    setSubmitting(true);
    setConfirm(false);
    for (let attempt = 0; ; attempt++) {
      try {
        await flushAll();
        const m = await submitMock(mock.id);
        savePending(mock.id, {});
        qc.setQueryData(keys.result(mock.id), m);
        void qc.invalidateQueries({ queryKey: keys.list(mock.courseId) });
        void qc.invalidateQueries({ queryKey: keys.mock(mock.id) });
        void navigate({ to: "/mock/$mockId/result", params: { mockId: mock.id }, replace: true });
        return;
      } catch {
        await new Promise((r) => setTimeout(r, Math.min(10_000, 1000 * 2 ** attempt)));
      }
    }
  }, [flushAll, mock.id, mock.courseId, navigate, qc]);

  // The countdown, from the server's clock; warn at 10 and 1 minute, hand in at 0.
  useEffect(() => {
    if (!mock.endsAt) return;
    const t = setInterval(() => {
      setLeft((before) => {
        const after = remaining(mock.endsAt, offset);
        const mark = crossed(before, after, mock.timeLimitMin);
        if (mark) toast({ message: mark === 10 ? "10 minutes left" : "1 minute left" });
        if (after === 0) void submit();
        return after;
      });
    }, 1000);
    return () => clearInterval(t);
  }, [mock.endsAt, mock.timeLimitMin, offset, submit, toast]);

  useBlocker({
    shouldBlockFn: () =>
      !done.current && !window.confirm("Leave the mock? Your answers are saved and the timer keeps running."),
    enableBeforeUnload: () => !done.current,
  });

  const go = (n: number) => setI(Math.max(0, Math.min(items.length - 1, n)));
  const next = () => (i === items.length - 1 ? setConfirm(true) : go(i + 1));

  // Keys (frontend-spec 2.6): digits choose with confidence, Space next, F flag.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (typingIn(e.target) || nav || confirm) return;
      const k = mockKey(e);
      if (!k) return;
      e.preventDefault();
      if (k.kind === "next") next();
      else if (k.kind === "flag") change(item.order, { flagged: !a?.flagged });
      else if (item.type === "MC") {
        const o = item.options?.[k.index] as { id: string } | undefined;
        if (o) change(item.order, { choiceOptionId: o.id, confidence: k.confidence });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const blanks = items.filter((it) => navState(answers[it.order], it.type) === "blank").length;
  const flagged = items.filter((it) => answers[it.order]?.flagged).length;

  return (
    <Shell
      mock={mock}
      right={
        <>
          <span className="hidden text-xs text-muted sm:inline" aria-live="polite">
            {save === "saved" ? "Saved" : save === "saving" ? "Saving…" : "Not saved yet, retrying"}
          </span>
          {left !== null ? (
            <span
              className={cn(
                "rounded-full border px-2.5 py-0.5 font-mono text-sm tnum",
                left <= 60_000
                  ? "border-danger text-danger-text"
                  : left <= 600_000 && (mock.timeLimitMin ?? 0) > 10
                    ? "border-warning text-warning-text"
                    : "border-border",
              )}
              role="timer"
              aria-label={`Time left ${clock(left)}`}
            >
              {clock(left)}
            </span>
          ) : null}
        </>
      }
    >
      <div className="h-[3px] bg-sunken" aria-hidden="true">
        <div
          className="h-full bg-[var(--course)] transition-[width]"
          style={{ width: `${((i + 1) / items.length) * 100}%` }}
        />
      </div>
      {submitting ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-3 p-8">
          <p className="m-0 text-lg font-medium">Submitting…</p>
          <p className="m-0 text-muted">Keep this page open; it retries until it's through.</p>
        </div>
      ) : (
        <>
          <div className="mx-auto flex w-full max-w-[720px] flex-1 flex-col gap-5 px-4 py-6">
            <div className="flex items-center gap-2 text-sm text-muted">
              Question {i + 1} of {items.length}
              {a?.flagged ? <Flag className="size-3.5 text-warning-text" aria-label="Flagged" /> : null}
            </div>
            <Markdown className="text-[17px]">{item.prompt}</Markdown>
            <AnswerControl item={item} a={a} onChange={(p) => change(item.order, p)} />
            <div className="flex flex-wrap items-center gap-3">
              <span className="text-sm text-muted">How sure?</span>
              <Segmented<Confidence>
                label="How sure are you?"
                size="sm"
                value={(a?.confidence as Confidence | null | undefined) ?? null}
                onChange={(confidence) => change(item.order, { confidence })}
                options={[
                  { value: "Sure", label: "Sure" },
                  { value: "Unsure", label: "Unsure" },
                  { value: "Guess", label: "Guess" },
                ]}
              />
            </div>
          </div>
          <footer className="sticky bottom-0 flex items-center gap-2 border-t border-border bg-bg px-4 py-3 pb-[max(12px,env(safe-area-inset-bottom))]">
            <Button
              size="sm"
              variant="ghost"
              aria-pressed={!!a?.flagged}
              onClick={() => change(item.order, { flagged: !a?.flagged })}
            >
              <Flag className="size-4" aria-hidden="true" />
              {a?.flagged ? "Flagged" : "Flag"}
            </Button>
            <Button size="sm" variant="ghost" onClick={() => setNav(true)}>
              <Grid3x3 className="size-4" aria-hidden="true" />
              All questions
            </Button>
            <span className="flex-1" />
            {i > 0 ? (
              <Button size="sm" onClick={() => go(i - 1)}>
                Back
              </Button>
            ) : null}
            <Button variant="primary" onClick={next}>
              {i === items.length - 1 ? "Finish" : "Next"}
            </Button>
          </footer>
        </>
      )}
      <Dialog
        open={nav}
        onOpenChange={setNav}
        title="All questions"
        description={`${blanks} blank · ${flagged} flagged`}
      >
        <div className="grid grid-cols-6 gap-2 sm:grid-cols-8">
          {items.map((it, n) => {
            const s = navState(answers[it.order], it.type);
            return (
              <button
                key={it.order}
                type="button"
                aria-label={`Question ${n + 1}, ${s}`}
                aria-current={n === i ? "step" : undefined}
                onClick={() => {
                  go(n);
                  setNav(false);
                }}
                className={cn(
                  "h-10 rounded-md border font-mono text-sm tnum",
                  s === "answered" && "border-text bg-sunken",
                  s === "flagged" && "border-warning bg-warning-bg text-warning-text",
                  s === "blank" && "border-dashed border-border text-muted",
                  n === i && "ring-2 ring-[var(--focus-ring)]",
                )}
              >
                {n + 1}
              </button>
            );
          })}
        </div>
        <div className="flex justify-end">
          <Button variant="primary" onClick={() => (setNav(false), setConfirm(true))}>
            Finish the mock
          </Button>
        </div>
      </Dialog>
      <Dialog
        open={confirm}
        onOpenChange={setConfirm}
        title="Hand it in?"
        description={
          blanks || flagged
            ? `${blanks ? `${blanks} blank` : ""}${blanks && flagged ? " and " : ""}${flagged ? `${flagged} flagged` : ""}. You can't change answers after this.`
            : "Every question has an answer. You can't change them after this."
        }
        footer={
          <>
            <Button onClick={() => setConfirm(false)}>Keep going</Button>
            <Button variant="primary" onClick={() => void submit()}>
              Hand in
            </Button>
          </>
        }
      />
    </Shell>
  );
}

function AnswerControl({
  item,
  a,
  onChange,
}: {
  item: Item;
  a: AnswerInput | undefined;
  onChange: (p: Partial<AnswerInput>) => void;
}) {
  if (item.type === "MC") {
    const options = (item.options ?? []) as { id: string; text: string }[];
    return (
      <div role="radiogroup" aria-label="Options" className="flex flex-col gap-2">
        {options.map((o, n) => (
          <OptionRow
            key={o.id}
            index={n}
            selected={a?.choiceOptionId === o.id}
            onSelect={() => onChange({ choiceOptionId: o.id })}
          >
            <Markdown>{o.text}</Markdown>
          </OptionRow>
        ))}
      </div>
    );
  }
  if (item.type === "Numeric") {
    return (
      <div className="flex flex-wrap items-center gap-2">
        <Input
          aria-label="Your answer"
          inputMode="decimal"
          className="h-11 w-48 font-mono text-md tnum"
          defaultValue={a?.numeric ?? ""}
          onChange={(e) => {
            const v = e.currentTarget.value.trim().replace(",", ".");
            const n = Number(v);
            onChange({ numeric: v && Number.isFinite(n) ? n : null });
          }}
        />
        {item.unit ? (
          <Input
            aria-label="Unit"
            className="h-11 w-28"
            placeholder={item.unit}
            defaultValue={a?.unit ?? ""}
            onChange={(e) => onChange({ unit: e.currentTarget.value.trim() || null })}
          />
        ) : null}
      </div>
    );
  }
  return (
    <Textarea
      aria-label="Your answer"
      className="min-h-48 text-md"
      defaultValue={a?.text ?? ""}
      onChange={(e) => onChange({ text: e.currentTarget.value })}
    />
  );
}

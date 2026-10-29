import { createFileRoute } from "@tanstack/react-router";
import { useEffect, useState, type FormEvent } from "react";
import { ConflictDialog } from "@/components/conflict-dialog";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Card, SectionLabel } from "@/components/ui/card";
import { Check, Field, Input, Select } from "@/components/ui/field";
import { Skeleton } from "@/components/ui/misc";
import { useToast } from "@/components/ui/toast";
import { usePatchSettings, type Settings } from "@/features/account/api";
import { fromSettings, TIMES, toPatch, type StudyForm as Form } from "@/features/account/lib/study";
import { useSettings } from "@/features/plan/api";
import { conflictCurrent, fieldErrors, messageFor } from "@/lib/errors";

// Study (frontend-spec 4): flashcard retention and limits, mock defaults, the study window
// (15-minute steps, at least 1 h, default 09:00–22:00), daily hours and one course a day. A planner
// setting rebuilds the plan at once (core does it on save).
export const Route = createFileRoute("/_app/settings/study")({ component: Study });

function StudyForm({ settings }: { settings: Settings }) {
  const patch = usePatchSettings();
  const toast = useToast();
  const [f, setF] = useState(() => fromSettings(settings));
  const [conflict, setConflict] = useState<Settings | null>(null);
  useEffect(() => setF(fromSettings(settings)), [settings]);
  const errors = fieldErrors(patch.error);
  const set = <K extends keyof Form>(k: K, v: Form[K]) => setF({ ...f, [k]: v });
  const dirty = JSON.stringify(f) !== JSON.stringify(fromSettings(settings));

  const send = (base: Settings) =>
    patch.mutate(
      { version: base.version, ...toPatch(f, base) },
      {
        onSuccess: () => toast({ message: "Saved" }),
        onError: (e) => {
          const cur = conflictCurrent<Settings>(e);
          if (cur) setConflict(cur);
        },
      },
    );
  const submit = (e: FormEvent) => {
    e.preventDefault();
    send(settings);
  };

  return (
    <form className="flex flex-col gap-5" onSubmit={submit} noValidate>
      {patch.error && !conflictCurrent(patch.error) && !Object.keys(errors).length ? (
        <Banner tone="danger">{messageFor(patch.error)}</Banner>
      ) : null}
      <Card className="flex flex-col gap-4">
        <SectionLabel>Planner</SectionLabel>
        <div className="grid gap-4 sm:grid-cols-3">
          <Field label="Study from" error={errors["extra.studyStart"]}>
            {(ids) => (
              <Select {...ids} value={f.studyStart} onChange={(e) => set("studyStart", e.currentTarget.value)}>
                {TIMES.slice(0, -1).map((t) => (
                  <option key={t}>{t}</option>
                ))}
              </Select>
            )}
          </Field>
          <Field label="Until" error={errors["extra.studyEnd"]}>
            {(ids) => (
              <Select {...ids} value={f.studyEnd} onChange={(e) => set("studyEnd", e.currentTarget.value)}>
                {TIMES.slice(1).map((t) => (
                  <option key={t}>{t}</option>
                ))}
              </Select>
            )}
          </Field>
          <Field label="Hours a day" error={errors["extra.dailyHours"]}>
            {(ids) => (
              <Input
                {...ids}
                type="number"
                inputMode="decimal"
                min={0.5}
                max={16}
                step={0.5}
                value={f.dailyHours}
                onChange={(e) => set("dailyHours", e.currentTarget.value)}
              />
            )}
          </Field>
        </div>
        <Check
          checked={f.oneCourse}
          onChange={(v) => set("oneCourse", v)}
          label="One course a day"
          hint="Turn off to mix courses within a day."
        />
        <p className="m-0 text-sm text-muted">
          Saving a change here rebuilds your plan. Entries you moved or pinned stay.
        </p>
      </Card>
      <Card className="flex flex-col gap-4">
        <SectionLabel>Flashcards</SectionLabel>
        <div className="grid gap-4 sm:grid-cols-3">
          <Field label="Target recall (%)" error={errors.flashRetention} hint="Higher means more reviews.">
            {(ids) => (
              <Input
                {...ids}
                type="number"
                inputMode="numeric"
                min={70}
                max={99}
                value={f.retention}
                onChange={(e) => set("retention", e.currentTarget.value)}
              />
            )}
          </Field>
          <Field label="New cards a day" error={errors.flashNewPerDay}>
            {(ids) => (
              <Input
                {...ids}
                type="number"
                inputMode="numeric"
                min={0}
                max={1000}
                value={f.newPerDay}
                onChange={(e) => set("newPerDay", e.currentTarget.value)}
              />
            )}
          </Field>
          <Field label="Reviews a day (most)" error={errors.flashReviewLimit}>
            {(ids) => (
              <Input
                {...ids}
                type="number"
                inputMode="numeric"
                min={0}
                max={10000}
                value={f.reviewLimit}
                onChange={(e) => set("reviewLimit", e.currentTarget.value)}
              />
            )}
          </Field>
        </div>
      </Card>
      <Card className="flex flex-col gap-4">
        <SectionLabel>Mocks</SectionLabel>
        <Check
          checked={f.realLayout}
          onChange={(v) => set("realLayout", v)}
          label="Use the real exam layout"
          hint="New mocks follow the exam's questions per lecture and open count."
        />
        <Check
          checked={f.goEasy}
          onChange={(v) => set("goEasy", v)}
          label="Go easy"
          hint="New mocks pick questions you've already got right less often."
        />
      </Card>
      <div className="flex gap-2">
        <Button type="submit" variant="primary" disabled={!dirty || patch.isPending}>
          Save
        </Button>
        {dirty ? (
          <Button onClick={() => setF(fromSettings(settings))} disabled={patch.isPending}>
            Cancel
          </Button>
        ) : null}
      </div>
      <ConflictDialog
        open={!!conflict}
        onClose={() => setConflict(null)}
        onReload={() => {
          if (conflict) setF(fromSettings(conflict));
          setConflict(null);
        }}
        onOverwrite={() => {
          if (conflict) send(conflict);
          setConflict(null);
        }}
      />
    </form>
  );
}

function Study() {
  const settings = useSettings();
  if (settings.isPending) return <Skeleton className="h-96" />;
  if (settings.isError) return <Banner tone="danger">{messageFor(settings.error)}</Banner>;
  return <StudyForm settings={settings.data} />;
}

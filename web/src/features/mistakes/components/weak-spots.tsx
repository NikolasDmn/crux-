import { Plus, Trash2 } from "lucide-react";
import { useState, type FormEvent } from "react";
import { useUser } from "@/app/auth";
import { TexMath } from "@/components/math";
import { Badge } from "@/components/ui/badge";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Card, SectionLabel } from "@/components/ui/card";
import { Field, Input, Select, Textarea } from "@/components/ui/field";
import { Dialog } from "@/components/ui/overlays";
import { SeverityBar, type Severity } from "@/components/ui/severity";
import { useToast } from "@/components/ui/toast";
import {
  SEVERITIES,
  useCreateWeakSpot,
  usePatchWeakSpot,
  useTrapActions,
  useTraps,
  useWeakSpots,
  type WeakSpot,
} from "@/features/mistakes/api";
import { fieldErrors, messageFor } from "@/lib/errors";
import { relativeDays } from "@/lib/time";

// Course overview (frontend-spec 4): the weak-spots table (topic, severity, last seen, cold passes,
// key rule as display maths), proposals from repeated slips, the resolved list, and traps.
export function WeakSpotsPanel({ courseId }: { courseId: string }) {
  const user = useUser();
  const spots = useWeakSpots(courseId);
  const create = useCreateWeakSpot(courseId);
  const toast = useToast();
  const [editing, setEditing] = useState<WeakSpot | "new" | null>(null);
  const [showResolved, setShowResolved] = useState(false);
  const data = spots.data;

  return (
    <Card className="flex flex-col gap-3">
      <div className="flex items-center justify-between">
        <SectionLabel>Weak spots</SectionLabel>
        <Button size="sm" variant="ghost" onClick={() => setEditing("new")}>
          <Plus className="size-4" aria-hidden="true" />
          Add
        </Button>
      </div>
      {data?.proposals.map((p) => (
        <Banner
          key={p.conceptName}
          tone="notice"
          action={
            <Button
              size="sm"
              disabled={create.isPending}
              onClick={() =>
                create.mutate(
                  { topic: p.conceptName, conceptId: p.conceptId ?? null },
                  { onSuccess: () => toast({ message: `${p.conceptName} is a weak spot now` }) },
                )
              }
            >
              Make it a weak spot
            </Button>
          }
        >
          {p.openSlips} open slips on {p.conceptName} in two weeks.
        </Banner>
      ))}
      {!data?.open.length ? (
        <p className="m-0 text-muted">None open. Misses in practice and mocks turn into weak spots here.</p>
      ) : (
        <ul className="m-0 flex list-none flex-col divide-y divide-border p-0">
          {data.open.map((w) => (
            <li key={w.id} className="flex flex-col gap-2 py-3">
              <button type="button" className="text-left" onClick={() => setEditing(w)}>
                <SeverityBar topic={w.topic} severity={w.severity as Severity} />
              </button>
              <div className="flex flex-wrap gap-x-4 gap-y-1 text-sm text-muted">
                <span>Last seen {relativeDays(w.lastSeenAt, user.timeZone)}</span>
                <span>
                  Cold passes {w.coldPassRun} of {w.coldPassesNeeded}
                </span>
                {w.status === "Reopened" ? (
                  <Badge kind="status" tone="warning">
                    Reopened
                  </Badge>
                ) : null}
              </div>
              {w.keyRule ? (
                <div className="overflow-x-auto rounded-md bg-sunken px-3 py-1">
                  <TexMath tex={w.keyRule} display />
                </div>
              ) : null}
              {w.detail ? <p className="m-0 text-sm">{w.detail}</p> : null}
            </li>
          ))}
        </ul>
      )}
      {data?.resolved.length ? (
        <div>
          <Button size="sm" variant="ghost" aria-expanded={showResolved} onClick={() => setShowResolved(!showResolved)}>
            {showResolved ? "Hide" : "Show"} resolved ({data.resolved.length})
          </Button>
          {showResolved ? (
            <ul className="m-0 mt-2 flex list-none flex-col gap-1 p-0 text-sm text-muted">
              {data.resolved.map((w) => (
                <li key={w.id}>
                  {w.topic} · resolved {w.resolvedAt ? relativeDays(w.resolvedAt, user.timeZone) : ""}
                </li>
              ))}
            </ul>
          ) : null}
        </div>
      ) : null}
      {editing ? (
        <SpotDialog
          courseId={courseId}
          spot={editing === "new" ? undefined : editing}
          onClose={() => setEditing(null)}
        />
      ) : null}
    </Card>
  );
}

function SpotDialog({
  courseId,
  spot,
  onClose,
}: {
  courseId: string;
  spot?: WeakSpot | undefined;
  onClose: () => void;
}) {
  const create = useCreateWeakSpot(courseId);
  const patch = usePatchWeakSpot(courseId);
  const m = spot ? patch : create;
  const [f, setF] = useState({
    topic: spot?.topic ?? "",
    severity: spot?.severity ?? "Medium",
    keyRule: spot?.keyRule ?? "",
    detail: spot?.detail ?? "",
    coldPassesNeeded: String(spot?.coldPassesNeeded ?? 2),
  });
  const errors = fieldErrors(m.error);
  const submit = (e: FormEvent) => {
    e.preventDefault();
    const body = {
      topic: f.topic.trim(),
      severity: f.severity,
      keyRule: f.keyRule.trim() || null,
      detail: f.detail.trim() || null,
      coldPassesNeeded: Number(f.coldPassesNeeded) || 2,
    };
    if (spot) patch.mutate({ spot, body }, { onSuccess: onClose });
    else create.mutate(body, { onSuccess: onClose });
  };
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()} title={spot ? "Weak spot" : "New weak spot"}>
      <form className="flex flex-col gap-4" onSubmit={submit} noValidate>
        {m.error && !Object.keys(errors).length ? <Banner tone="danger">{messageFor(m.error)}</Banner> : null}
        <Field label="Topic" error={errors.topic}>
          {(ids) => (
            <Input {...ids} autoFocus value={f.topic} onChange={(e) => setF({ ...f, topic: e.currentTarget.value })} />
          )}
        </Field>
        <div className="grid gap-4 sm:grid-cols-2">
          <Field
            label="Severity"
            error={errors.severity}
            hint={spot ? "Changing it restarts the miss count." : undefined}
          >
            {(ids) => (
              <Select {...ids} value={f.severity} onChange={(e) => setF({ ...f, severity: e.currentTarget.value })}>
                {SEVERITIES.map((s) => (
                  <option key={s}>{s}</option>
                ))}
              </Select>
            )}
          </Field>
          <Field label="Cold passes to resolve" error={errors.coldPassesNeeded}>
            {(ids) => (
              <Input
                {...ids}
                inputMode="numeric"
                className="font-mono tnum"
                value={f.coldPassesNeeded}
                onChange={(e) => setF({ ...f, coldPassesNeeded: e.currentTarget.value.replace(/\D/g, "").slice(0, 2) })}
              />
            )}
          </Field>
        </div>
        <Field label="Key rule (TeX)" error={errors.keyRule}>
          {(ids) => (
            <Input
              {...ids}
              className="font-mono text-sm"
              placeholder="s^2 = \frac{1}{n-1}\sum (x_i-\bar x)^2"
              value={f.keyRule}
              onChange={(e) => setF({ ...f, keyRule: e.currentTarget.value })}
            />
          )}
        </Field>
        {f.keyRule.trim() ? (
          <div className="overflow-x-auto rounded-md bg-sunken px-3 py-1">
            <TexMath tex={f.keyRule} display />
          </div>
        ) : null}
        <Field label="Notes" error={errors.detail}>
          {(ids) => (
            <Textarea {...ids} value={f.detail} onChange={(e) => setF({ ...f, detail: e.currentTarget.value })} />
          )}
        </Field>
        <div className="flex justify-end gap-2">
          <Button onClick={onClose}>Cancel</Button>
          <Button type="submit" variant="primary" disabled={m.isPending || !f.topic.trim()}>
            Save
          </Button>
        </div>
      </form>
    </Dialog>
  );
}

/** Traps: one-line reminders shown before every mock ("units in kN, not N"). */
export function TrapsPanel({ courseId }: { courseId: string }) {
  const traps = useTraps(courseId);
  const { add, remove } = useTrapActions(courseId);
  const [text, setText] = useState("");
  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (!text.trim()) return;
    add.mutate({ text: text.trim() }, { onSuccess: () => setText("") });
  };
  return (
    <Card className="flex flex-col gap-3">
      <SectionLabel>Traps</SectionLabel>
      <p className="m-0 text-sm text-muted">Shown on the start screen of every mock.</p>
      {traps.data?.length ? (
        <ul className="m-0 flex list-none flex-col gap-1 p-0">
          {traps.data.map((t) => (
            <li key={t.id} className="flex items-center gap-2 text-sm">
              <span className="min-w-0 flex-1">{t.text}</span>
              <Button
                size="iconSm"
                variant="ghost"
                aria-label={`Remove trap ${t.text}`}
                disabled={remove.isPending}
                onClick={() => remove.mutate(t.id)}
              >
                <Trash2 className="size-4" />
              </Button>
            </li>
          ))}
        </ul>
      ) : null}
      <form className="flex gap-2" onSubmit={submit}>
        <Input
          aria-label="New trap"
          placeholder="Read whether they ask for the sample or the population"
          value={text}
          onChange={(e) => setText(e.currentTarget.value)}
        />
        <Button type="submit" size="sm" disabled={add.isPending || !text.trim()}>
          Add
        </Button>
      </form>
    </Card>
  );
}

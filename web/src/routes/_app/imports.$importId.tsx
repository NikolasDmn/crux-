import { useQueries } from "@tanstack/react-query";
import { createFileRoute, Link, useNavigate } from "@tanstack/react-router";
import { CircleAlert, CircleCheck, CircleDot, Plus, X } from "lucide-react";
import { useState } from "react";
import { Markdown } from "@/components/markdown";
import { Badge } from "@/components/ui/badge";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Card, SectionLabel } from "@/components/ui/card";
import { Field, Input, Textarea } from "@/components/ui/field";
import { Skeleton } from "@/components/ui/misc";
import { Dialog } from "@/components/ui/overlays";
import { ProgressBar } from "@/components/ui/progress";
import { useToast } from "@/components/ui/toast";
import {
  useImport,
  useImportActions,
  useImportStream,
  useProposals,
  type ImportView,
  type ProposalView,
} from "@/features/ai/api";
import {
  applyBlocker,
  defaultSelected,
  EDIT_FIELDS,
  editsFor,
  effective,
  KIND_TITLE,
  optionsProblem,
  readField,
  readMulti,
  readOptions,
  selected,
  type OptionEdit,
  STATUS_TEXT,
  summary,
  working,
} from "@/features/ai/lib/imports";
import { useLectures } from "@/features/lectures/api";
import { fileUrl, getFile } from "@/features/files/api";
import { sameOrigin } from "@/features/files/lib/upload";
import { fieldErrors, messageFor } from "@/lib/errors";
import { cn } from "@/lib/cn";

// Import review (frontend-spec 4, 5.6): live progress over SSE, then proposals grouped by kind, each
// with its source file and page, confidence as a word and icon, the rationale and a duplicate badge.
// Accept, edit or reject one, or a whole group; "Apply accepted" adds them, then opens the course.
export const Route = createFileRoute("/_app/imports/$importId")({ component: ImportReview });

const CONFIDENCE = {
  High: { icon: CircleCheck, tone: "success" },
  Medium: { icon: CircleDot, tone: "info" },
  Low: { icon: CircleAlert, tone: "warning" },
} as const;

function ImportReview() {
  const { importId } = Route.useParams();
  const imp = useImport(importId);
  const status = imp.data?.status;
  useImportStream(importId, !!status && working(status));
  const proposals = useProposals(importId, !!status && !working(status) && status !== "Cancelled");

  if (imp.isError) return <Banner tone="danger">{messageFor(imp.error)}</Banner>;
  if (!imp.data) return <Skeleton className="h-96" />;
  const v = imp.data;
  return (
    <div className="flex max-w-[860px] flex-col gap-5">
      <div className="flex flex-col gap-1">
        <h1 className="m-0 text-xl font-semibold tracking-[-0.01em]">
          {v.courseId ? "Import into your course" : "Set up a course with AI"}
        </h1>
        <span className="text-sm text-muted">
          {STATUS_TEXT[v.status] ?? v.status} · {v.files.length} {v.files.length === 1 ? "file" : "files"}
        </span>
      </div>
      {working(v.status) ? <Progress v={v} /> : null}
      {v.status === "Failed" ? <Failed v={v} /> : null}
      {v.status === "Cancelled" ? <Banner tone="notice">This import was cancelled.</Banner> : null}
      {v.truncated ? (
        <Banner tone="warning">Only the first part of your files was read. Import the rest separately.</Banner>
      ) : null}
      {v.status === "Applied" ? <Applied v={v} /> : null}
      {proposals.data && (v.status === "Proposed" || v.status === "Applied") ? (
        <Review v={v} groups={proposals.data} />
      ) : proposals.isPending && !working(v.status) && v.status !== "Cancelled" ? (
        <Skeleton className="h-64" />
      ) : null}
    </div>
  );
}

function Progress({ v }: { v: ImportView }) {
  const { cancel } = useImportActions(v.id);
  const toast = useToast();
  const done = v.files.filter((f) => f.status !== "Pending").length;
  return (
    <Card className="flex flex-col gap-3">
      <ProgressBar
        tone="text"
        value={v.status === "Extracting" ? Math.max(done, 0.3) : 0.1}
        max={Math.max(1, v.files.length)}
        label={STATUS_TEXT[v.status]}
        detail={v.parts && v.parts > 1 ? `${v.parts} parts` : undefined}
      />
      <p className="m-0 text-sm text-muted">
        This takes a minute or two. You can leave this page; the inbox tells you when it's ready.
      </p>
      {v.status !== "Applying" ? (
        <div>
          <Button
            size="sm"
            disabled={cancel.isPending}
            onClick={() =>
              cancel.mutate(undefined, { onError: (e) => toast({ message: messageFor(e), tone: "danger" }) })
            }
          >
            Cancel
          </Button>
        </div>
      ) : null}
    </Card>
  );
}

function Failed({ v }: { v: ImportView }) {
  const { retry } = useImportActions(v.id);
  const toast = useToast();
  return (
    <Banner tone="danger">
      <span className="flex flex-wrap items-center gap-3">
        <span className="flex-1">The AI couldn't read your files this time.</span>
        <Button
          size="sm"
          disabled={retry.isPending}
          onClick={() => retry.mutate(undefined, { onError: (e) => toast({ message: messageFor(e), tone: "danger" }) })}
        >
          Try again
        </Button>
      </span>
    </Banner>
  );
}

function Applied({ v }: { v: ImportView }) {
  return (
    <Banner tone="notice">
      <span className="flex flex-wrap items-center gap-3">
        <span className="flex-1">Added to your course.</span>
        {v.courseId ? (
          <Link to="/c/$courseId" params={{ courseId: v.courseId }} className="text-sm underline underline-offset-4">
            Open the course
          </Link>
        ) : null}
      </span>
    </Banner>
  );
}

function Review({ v, groups }: { v: ImportView; groups: Record<string, ProposalView[]> }) {
  const navigate = useNavigate();
  const toast = useToast();
  const { bulk, apply } = useImportActions(v.id);
  const open = v.status === "Proposed";
  const all = Object.values(groups).flat();
  const blocker = applyBlocker(!v.courseId, all);
  const names = useFileNames(v.files.map((f) => f.fileId));
  const lectures = useLectures(v.courseId ?? "");
  const onError = (e: unknown) => toast({ message: messageFor(e), tone: "danger" });
  if (!all.length)
    return (
      <Banner tone="notice">The AI found nothing to propose in these files. Try other files or instructions.</Banner>
    );

  const runApply = async () => {
    try {
      // Pending proposals that start selected (5.6) are accepted first; apply takes Accepted/Edited.
      const pending = all.filter((p) => p.status === "Pending" && defaultSelected(p)).map((p) => p.id);
      if (pending.length) await bulk.mutateAsync({ action: "accept", proposalIds: pending });
      const done = await apply.mutateAsync();
      toast({ message: "Added to your course" });
      if (done.courseId) void navigate({ to: "/c/$courseId", params: { courseId: done.courseId } });
    } catch (e) {
      onError(e);
    }
  };

  return (
    <>
      {Object.entries(groups)
        .filter(([, list]) => list.length)
        .map(([kind, list]) => (
          <section key={kind} className="flex flex-col gap-2">
            <div className="flex items-center gap-3">
              <SectionLabel className="flex-1">
                {KIND_TITLE[kind] ?? kind} ({list.length})
              </SectionLabel>
              {open && list.length > 1 ? (
                <>
                  <Button
                    size="sm"
                    variant="ghost"
                    disabled={bulk.isPending}
                    onClick={() => bulk.mutate({ action: "accept", proposalIds: list.map((p) => p.id) }, { onError })}
                  >
                    Select all
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    disabled={bulk.isPending}
                    onClick={() => bulk.mutate({ action: "reject", proposalIds: list.map((p) => p.id) }, { onError })}
                  >
                    None
                  </Button>
                </>
              ) : null}
            </div>
            <ul className="m-0 flex list-none flex-col gap-2 p-0">
              {list.map((p) => (
                <ProposalCard
                  key={p.id}
                  importId={v.id}
                  p={p}
                  open={open}
                  courseId={v.courseId}
                  fileName={p.sourceFileId ? names.get(p.sourceFileId) : undefined}
                  dupName={
                    p.kind === "Component" && p.duplicateOfId
                      ? lectures.data?.find((l) => l.id === p.duplicateOfId)
                      : undefined
                  }
                />
              ))}
            </ul>
          </section>
        ))}
      {open ? (
        <div className="sticky bottom-0 -mx-1 flex flex-wrap items-center gap-3 border-t border-border bg-bg/95 px-1 py-3 backdrop-blur">
          <span className="flex-1 text-sm text-muted">
            {all.filter(selected).length} of {all.length} selected
            {blocker ? ` · ${blocker}` : ""}
          </span>
          <Button
            variant="primary"
            disabled={!!blocker || apply.isPending || bulk.isPending}
            onClick={() => void runApply()}
          >
            {apply.isPending ? "Adding…" : "Apply accepted"}
          </Button>
        </div>
      ) : null}
    </>
  );
}

/** File names for "From: file.pdf p.4". */
function useFileNames(ids: string[]) {
  const files = useQueries({
    queries: ids.map((id) => ({ queryKey: ["file", id], queryFn: () => getFile(id), staleTime: 5 * 60_000 })),
  });
  return new Map(files.flatMap((f) => (f.data ? [[f.data.id, f.data.fileName] as const] : [])));
}

async function openSource(fileId: string, page: number | null) {
  // A new tab first (popup blockers allow it on the click), then its address once the URL is signed.
  const tab = window.open("", "_blank");
  try {
    const { url } = await fileUrl(fileId);
    const href = `${sameOrigin(url, location.origin)}${page ? `#page=${page}` : ""}`;
    if (tab) tab.location.href = href;
    else window.location.href = href;
  } catch {
    tab?.close();
  }
}

function ProposalCard({
  importId,
  p,
  open,
  courseId,
  fileName,
  dupName,
}: {
  importId: string;
  p: ProposalView;
  open: boolean;
  courseId: string | null;
  fileName: string | undefined;
  dupName: { code: string; name: string } | undefined;
}) {
  const { patch } = useImportActions(importId);
  const toast = useToast();
  const [editing, setEditing] = useState(false);
  const payload = effective(p);
  const s = summary(p.kind, payload);
  const on = selected(p);
  const conf = CONFIDENCE[p.confidence as keyof typeof CONFIDENCE];
  const Icon = conf?.icon ?? CircleDot;
  const set = (status: "Accepted" | "Rejected") =>
    patch.mutate(
      { proposalId: p.id, body: { status } },
      { onError: (e) => toast({ message: messageFor(e), tone: "danger" }) },
    );
  const dupHref =
    courseId && p.duplicateOfId
      ? p.kind === "Component"
        ? `/c/${courseId}/l/${p.duplicateOfId}`
        : p.kind === "Question"
          ? `/c/${courseId}/q/${p.duplicateOfId}`
          : p.kind === "Paper"
            ? `/c/${courseId}/papers`
            : null
      : null;
  const content = (payload.content ?? {}) as {
    options?: { text?: string; isCorrect?: boolean }[];
    multiSelect?: boolean;
  };

  return (
    <li
      className={cn(
        "flex gap-3 rounded-lg border border-border bg-surface p-3",
        // Not chosen: a dashed outline and tint, not opacity (that dropped the text below contrast).
        !on && "border-dashed bg-sunken/50",
        p.status === "Failed" && "border-danger/50",
      )}
    >
      {open ? (
        <input
          type="checkbox"
          className="mt-1 size-4 shrink-0 accent-[var(--text)]"
          aria-label={`Include ${s.title || p.kind}`}
          checked={on}
          disabled={patch.isPending}
          onChange={(e) => set(e.currentTarget.checked ? "Accepted" : "Rejected")}
        />
      ) : null}
      <div className="flex min-w-0 flex-1 flex-col gap-1.5">
        <div className="flex flex-wrap items-center gap-2">
          {p.kind === "Question" ? null : <span className="min-w-0 font-medium">{s.title || "(untitled)"}</span>}
          <Badge tone={conf?.tone ?? "neutral"} className="inline-flex items-center gap-1">
            <Icon className="size-3" aria-hidden="true" />
            {p.confidence}
          </Badge>
          {p.status === "Edited" ? <Badge kind="label">Edited</Badge> : null}
          {p.status === "Applied" ? <Badge tone="success">Added</Badge> : null}
          {p.status === "Failed" ? <Badge tone="danger">Not added</Badge> : null}
          {p.duplicateOfId ? (
            dupHref ? (
              <Link to={dupHref} className="text-sm underline underline-offset-4">
                {dupName ? `Matches your ${dupName.code} ${dupName.name}` : "Matches one you have"}
              </Link>
            ) : (
              <Badge kind="label">Matches one you have</Badge>
            )
          ) : null}
        </div>
        {p.kind === "Question" ? (
          <>
            <Markdown>{s.title}</Markdown>
            {content.options?.length && content.multiSelect ? (
              <span className="text-sm text-muted">Choose all that apply</span>
            ) : null}
            {content.options?.length ? (
              <ul className="m-0 flex list-none flex-col gap-1 p-0 text-sm">
                {content.options.map((o, i) => (
                  <li key={i} className={cn(o.isCorrect && "font-medium")}>
                    {o.isCorrect ? "✓ " : "· "}
                    {o.text}
                  </li>
                ))}
              </ul>
            ) : null}
          </>
        ) : null}
        {s.detail ? <span className="text-sm text-muted">{s.detail}</span> : null}
        {p.rationale ? <span className="text-sm text-muted">{p.rationale}</span> : null}
        <div className="flex flex-wrap items-center gap-3 text-sm">
          {p.sourceFileId ? (
            <button
              type="button"
              className="text-muted underline-offset-4 hover:underline"
              onClick={() => void openSource(p.sourceFileId!, p.sourcePage ?? null)}
            >
              From: {fileName ?? "your file"}
              {p.sourcePage ? ` p.${p.sourcePage}` : ""}
            </button>
          ) : null}
          {open && EDIT_FIELDS[p.kind] ? (
            <Button size="sm" variant="ghost" onClick={() => setEditing(true)}>
              Edit
            </Button>
          ) : null}
        </div>
      </div>
      {editing ? <EditDialog importId={importId} p={p} onClose={() => setEditing(false)} /> : null}
    </li>
  );
}

function EditDialog({ importId, p, onClose }: { importId: string; p: ProposalView; onClose: () => void }) {
  const { patch } = useImportActions(importId);
  const payload = effective(p);
  const fields = EDIT_FIELDS[p.kind] ?? [];
  const [values, setValues] = useState(() => Object.fromEntries(fields.map((f) => [f.key, readField(payload, f.key)])));
  // An MC question's options: text, which one is right, add and remove.
  const [options, setOptions] = useState<OptionEdit[]>(() => (p.kind === "Question" ? readOptions(payload) : []));
  const [multi, setMulti] = useState(() => readMulti(payload));
  const problem = optionsProblem(options, multi);
  const errors = fieldErrors(patch.error);
  // One correct option clears the others, unless it's "choose all that apply".
  const setOption = (i: number, o: Partial<OptionEdit>) =>
    setOptions(
      options.map((x, n) => (n === i ? { ...x, ...o } : o.isCorrect && !multi ? { ...x, isCorrect: false } : x)),
    );
  const toggleMulti = (on: boolean) => {
    setMulti(on);
    // Back to one answer: keep only the first correct option.
    if (!on) {
      const first = options.findIndex((o) => o.isCorrect);
      setOptions(options.map((o, n) => ({ ...o, isCorrect: n === first })));
    }
  };
  const save = () => {
    if (problem) return;
    const edits = editsFor(
      p.kind,
      payload,
      values,
      options.length ? options : undefined,
      options.length ? multi : undefined,
    );
    if (!Object.keys(edits).length) return onClose();
    patch.mutate({ proposalId: p.id, body: { edits, status: "Accepted" } }, { onSuccess: onClose });
  };
  return (
    <Dialog
      open
      onOpenChange={(o) => !o && onClose()}
      title={`Edit ${(KIND_TITLE[p.kind] ?? p.kind).toLowerCase().replace(/s$/, "")}`}
      description="Saving also selects it."
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={patch.isPending || !!problem} onClick={save}>
            Save
          </Button>
        </>
      }
    >
      {patch.error && !Object.keys(errors).length ? <Banner tone="danger">{messageFor(patch.error)}</Banner> : null}
      {fields.map((f) => (
        <Field key={f.key} label={f.label}>
          {(ids) =>
            f.type === "textarea" ? (
              <Textarea
                {...ids}
                rows={4}
                value={values[f.key] ?? ""}
                onChange={(e) => setValues({ ...values, [f.key]: e.currentTarget.value })}
              />
            ) : (
              <Input
                {...ids}
                type={f.type}
                value={values[f.key] ?? ""}
                onChange={(e) => setValues({ ...values, [f.key]: e.currentTarget.value })}
              />
            )
          }
        </Field>
      ))}
      {options.length ? (
        <fieldset className="m-0 flex flex-col gap-2 border-0 p-0">
          <legend className="mb-1.5 text-sm font-medium">Options</legend>
          <label className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              className="size-4 accent-[var(--primary)]"
              checked={multi}
              onChange={(e) => toggleMulti(e.currentTarget.checked)}
            />
            Choose all that apply
          </label>
          <div
            role={multi ? "group" : "radiogroup"}
            aria-label={multi ? "Correct options" : "Correct option"}
            className="flex flex-col gap-2"
          >
            {options.map((o, i) => {
              const letter = String.fromCharCode(65 + i);
              return (
                <div key={i} className="flex items-center gap-2">
                  <input
                    type={multi ? "checkbox" : "radio"}
                    name="correct"
                    className="size-4 accent-[var(--primary)]"
                    aria-label={`Option ${letter} is correct`}
                    checked={o.isCorrect}
                    onChange={() => setOption(i, { isCorrect: multi ? !o.isCorrect : true })}
                  />
                  <Input
                    aria-label={`Option ${letter}`}
                    value={o.text}
                    onChange={(e) => setOption(i, { text: e.currentTarget.value })}
                  />
                  <Button
                    size="iconSm"
                    variant="ghost"
                    aria-label={`Remove option ${letter}`}
                    disabled={options.length <= 2}
                    onClick={() => setOptions(options.filter((_, n) => n !== i))}
                  >
                    <X />
                  </Button>
                </div>
              );
            })}
          </div>
          <div className="flex items-center gap-3">
            <Button
              size="sm"
              variant="ghost"
              disabled={options.length >= 8}
              onClick={() => setOptions([...options, { text: "", isCorrect: false }])}
            >
              <Plus aria-hidden="true" />
              Add option
            </Button>
            {problem ? <span className="text-sm text-danger-text">{problem}</span> : null}
          </div>
        </fieldset>
      ) : null}
    </Dialog>
  );
}

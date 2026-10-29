import { useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { FileText, MoreHorizontal, NotebookText, X } from "lucide-react";
import { useState } from "react";
import { Markdown } from "@/components/markdown";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { DropBox, EmptyList } from "@/components/ui/empty";
import { Field, Input, Select, Textarea } from "@/components/ui/field";
import { Dialog, Menu, MenuContent, MenuItem, MenuTrigger } from "@/components/ui/overlays";
import { useToast } from "@/components/ui/toast";
import { markRestored, RESTORED, useUploads } from "@/features/files/api";
import { ACCEPT, formatBytes, phaseLabel } from "@/features/files/lib/upload";
import {
  useCreateMaterial,
  useDeleteMaterial,
  useMaterials,
  usePatchMaterial,
  type Material,
} from "@/features/lectures/api";
import { idempotencyKey } from "@/lib/api";
import { fieldErrors, messageFor } from "@/lib/errors";
import { cn } from "@/lib/cn";

const KINDS = ["Slides", "Textbook", "Exercise", "PastExam", "Notes", "Other"] as const;
const KIND_LABEL: Record<string, string> = { PastExam: "Past exam" };

function MaterialDialog({
  componentId,
  material,
  open,
  onClose,
}: {
  componentId: string;
  material: Material | "new-notes" | null;
  open: boolean;
  onClose: () => void;
}) {
  const isNew = material === "new-notes";
  const m = isNew ? null : material;
  const create = useCreateMaterial(componentId);
  const patch = usePatchMaterial(componentId);
  const [key] = useState(idempotencyKey);
  const [f, setF] = useState({
    title: m?.title ?? "",
    kind: m?.kind ?? "Notes",
    content: m?.content ?? "",
    excluded: m?.excluded ?? false,
    excludedNote: m?.excludedNote ?? "",
    aiAllowed: m?.aiAllowed ?? true,
  });
  const err = isNew ? create.error : patch.error;
  const errors = fieldErrors(err);
  const notes = isNew || (m && !m.fileId);
  const save = () => {
    const fields = {
      title: f.title.trim(),
      kind: f.kind,
      excluded: f.excluded,
      excludedNote: f.excluded ? f.excludedNote.trim() || null : null,
      aiAllowed: f.aiAllowed,
      ...(notes ? { content: f.content } : {}),
    };
    if (isNew) create.mutate({ key, fields }, { onSuccess: onClose });
    else if (m) patch.mutate({ id: m.id, version: m.version, fields }, { onSuccess: onClose });
  };
  return (
    <Dialog
      open={open}
      onOpenChange={(o) => !o && onClose()}
      title={isNew ? "New notes" : `Edit ${m?.title ?? ""}`}
      className={cn(notes && "w-[min(900px,calc(100vw-32px))]")}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={!f.title.trim() || create.isPending || patch.isPending} onClick={save}>
            Save
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        {err && !Object.keys(errors).length ? <p className="m-0 text-danger-text">{messageFor(err)}</p> : null}
        <div className="grid gap-3 sm:grid-cols-[1fr_160px]">
          <Field label="Title" error={errors.title}>
            {(ids) => (
              <Input
                {...ids}
                autoFocus
                value={f.title}
                onChange={(e) => setF({ ...f, title: e.currentTarget.value })}
              />
            )}
          </Field>
          <Field label="Kind">
            {(ids) => (
              <Select {...ids} value={f.kind} onChange={(e) => setF({ ...f, kind: e.currentTarget.value })}>
                {KINDS.map((k) => (
                  <option key={k} value={k}>
                    {KIND_LABEL[k] ?? k}
                  </option>
                ))}
              </Select>
            )}
          </Field>
        </div>
        {notes ? (
          <div className="grid gap-3 md:grid-cols-2">
            <Field label="Notes (markdown, $maths$)" error={errors.content}>
              {(ids) => (
                <Textarea
                  {...ids}
                  className="min-h-72 font-mono text-sm"
                  value={f.content}
                  onChange={(e) => setF({ ...f, content: e.currentTarget.value })}
                />
              )}
            </Field>
            <div className="flex flex-col gap-1.5">
              <span className="text-sm font-medium">Preview</span>
              <div className="min-h-72 overflow-auto rounded-md border border-border bg-bg p-3">
                <Markdown>{f.content || "_Nothing yet._"}</Markdown>
              </div>
            </div>
          </div>
        ) : null}
        <label className="flex items-center gap-2">
          <input
            type="checkbox"
            checked={f.excluded}
            onChange={(e) => setF({ ...f, excluded: e.currentTarget.checked })}
            className="size-4 accent-[var(--primary)]"
          />
          Not part of the exam (excluded)
        </label>
        {f.excluded ? (
          <Field label="Why excluded">
            {(ids) => (
              <Input
                {...ids}
                placeholder="Optional, e.g. not in this year's syllabus"
                value={f.excludedNote}
                onChange={(e) => setF({ ...f, excludedNote: e.currentTarget.value })}
              />
            )}
          </Field>
        ) : null}
        <label className="flex items-center gap-2">
          <input
            type="checkbox"
            checked={!f.aiAllowed}
            onChange={(e) => setF({ ...f, aiAllowed: !e.currentTarget.checked })}
            className="size-4 accent-[var(--primary)]"
          />
          Keep this away from AI (licence or copyright)
        </label>
      </div>
    </Dialog>
  );
}

/** A lecture's materials (frontend-spec 4): PDFs and notes, badges for excluded / AI-blocked /
 *  original removed; Editors upload (dashed drop box), write notes, edit and delete. */
export function MaterialList({
  courseId,
  componentId,
  editor,
}: {
  courseId: string;
  componentId: string;
  editor: boolean;
}) {
  const materials = useMaterials(componentId);
  const create = useCreateMaterial(componentId);
  const remove = useDeleteMaterial(componentId);
  const toast = useToast();
  const [editing, setEditing] = useState<Material | "new-notes" | null>(null);
  const qc = useQueryClient();
  const { uploads, add, dismiss } = useUploads({
    courseId,
    onClean: (file, name) => {
      // The same bytes as a removed original: files hands back that file, already attached here.
      if (materials.data?.some((m) => m.fileId === file.id)) {
        markRestored(qc, file.id);
        toast({ message: RESTORED });
        return;
      }
      create.mutate(
        { key: idempotencyKey(), fields: { fileId: file.id, title: name.replace(/\.pdf$/i, ""), kind: "Slides" } },
        { onError: (e) => toast({ message: messageFor(e), tone: "danger" }) },
      );
    },
  });

  const list = materials.data ?? [];
  return (
    <div className="flex flex-col gap-4">
      {list.length ? (
        <ul className="m-0 flex list-none flex-col divide-y divide-border rounded-lg border border-border bg-surface p-0">
          {list.map((m) => (
            <li key={m.id} className="flex items-center gap-3 px-4 py-3">
              {m.fileId ? (
                <FileText className="size-4 shrink-0 text-muted" aria-hidden="true" />
              ) : (
                <NotebookText className="size-4 shrink-0 text-muted" aria-hidden="true" />
              )}
              <Link
                to="/c/$courseId/m/$materialId"
                params={{ courseId, materialId: m.id }}
                className="min-w-0 flex-1 truncate font-medium underline-offset-4 hover:underline"
              >
                {m.title}
              </Link>
              <div className="flex flex-wrap justify-end gap-1.5">
                <Badge kind="label">{KIND_LABEL[m.kind] ?? m.kind}</Badge>
                {m.excluded ? <Badge tone="warning">Excluded</Badge> : null}
                {!m.aiAllowed ? <Badge>AI-blocked</Badge> : null}
                {m.originalStatus === "Purged" ? <Badge tone="info">Original removed</Badge> : null}
                {m.originalStatus === "PendingPurge" ? <Badge tone="info">Removing original</Badge> : null}
              </div>
              {editor ? (
                <Menu>
                  <MenuTrigger asChild>
                    <Button size="iconSm" variant="ghost" aria-label={`More for ${m.title}`}>
                      <MoreHorizontal />
                    </Button>
                  </MenuTrigger>
                  <MenuContent align="end">
                    <MenuItem onSelect={() => setEditing(m)}>Edit</MenuItem>
                    <MenuItem
                      className="text-danger-text"
                      onSelect={() =>
                        remove.mutate(m.id, { onError: (e) => toast({ message: messageFor(e), tone: "danger" }) })
                      }
                    >
                      Delete
                    </MenuItem>
                  </MenuContent>
                </Menu>
              ) : null}
            </li>
          ))}
        </ul>
      ) : !editor ? (
        <EmptyList
          title="No materials yet"
          body="Slides and notes for this lecture show up here once the course owner adds them."
        />
      ) : null}

      {uploads.length ? (
        <ul className="m-0 flex list-none flex-col gap-2 p-0" aria-live="polite">
          {uploads.map((u) => (
            <li
              key={u.key}
              className="flex items-center gap-3 rounded-md border border-border bg-surface px-3 py-2 text-sm"
            >
              <span className="min-w-0 flex-1 truncate">{u.name}</span>
              <span className="font-mono text-xs text-muted">{formatBytes(u.size)}</span>
              <span
                className={cn(
                  "w-48 text-right",
                  u.state.phase === "failed"
                    ? "text-danger-text"
                    : u.state.phase === "ready"
                      ? "text-success-text"
                      : "text-muted",
                )}
              >
                {phaseLabel(u.state)}
                {u.state.phase === "failed" && u.state.quota ? (
                  <>
                    {" "}
                    <Link to="/settings/storage" className="underline underline-offset-4">
                      Storage
                    </Link>
                  </>
                ) : null}
              </span>
              {u.state.phase === "uploading" ? (
                <span className="h-1 w-20 overflow-hidden rounded-full bg-sunken" aria-hidden="true">
                  <span
                    className="block h-full bg-course"
                    style={{ width: `${u.state.total ? (u.state.loaded / u.state.total) * 100 : 0}%` }}
                  />
                </span>
              ) : null}
              {u.state.phase === "ready" || u.state.phase === "failed" ? (
                <Button size="iconSm" variant="ghost" aria-label={`Dismiss ${u.name}`} onClick={() => dismiss(u.key)}>
                  <X />
                </Button>
              ) : null}
            </li>
          ))}
        </ul>
      ) : null}

      {editor ? (
        <div className="grid gap-3 md:grid-cols-[1fr_auto] md:items-stretch">
          <DropBox
            title="Drop lecture slides here"
            hint="PDFs up to 100 MB, or click to choose"
            accept={ACCEPT}
            multiple
            onFiles={add}
          />
          <Button className="h-auto min-h-11 px-6" onClick={() => setEditing("new-notes")}>
            <NotebookText aria-hidden="true" />
            Write notes
          </Button>
        </div>
      ) : null}

      {editing ? (
        <MaterialDialog
          key={editing === "new-notes" ? "new" : editing.id}
          componentId={componentId}
          material={editing}
          open
          onClose={() => setEditing(null)}
        />
      ) : null}
    </div>
  );
}

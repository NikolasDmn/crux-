import { useState, type FormEvent } from "react";
import { Markdown } from "@/components/markdown";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Field, Select, Textarea } from "@/components/ui/field";
import { Dialog } from "@/components/ui/overlays";
import { useToast } from "@/components/ui/toast";
import { useCourse } from "@/features/courses/api";
import { canEdit } from "@/features/courses/lib/roles";
import { useCreateCard, usePatchCard, type Card } from "@/features/flash/api";
import { useLectures } from "@/features/lectures/api";
import { conflictCurrent, fieldErrors, messageFor } from "@/lib/errors";

export interface CardDraft {
  componentId?: string | undefined;
  front?: string | undefined;
  back?: string | undefined;
}

/** New card (personal, or a course card for Owners and Editors) or edit one; markdown both sides. */
export function CardDialog({
  courseId,
  card,
  draft,
  onClose,
}: {
  courseId: string;
  card?: Card | undefined;
  draft?: CardDraft | undefined;
  onClose: () => void;
}) {
  const course = useCourse(courseId);
  const editor = canEdit(course.data?.myRole);
  const lectures = useLectures(courseId);
  const create = useCreateCard(courseId);
  const patch = usePatchCard();
  const toast = useToast();
  const [f, setF] = useState({
    componentId: card?.componentId ?? draft?.componentId ?? "",
    front: card?.front ?? draft?.front ?? "",
    back: card?.back ?? draft?.back ?? "",
    shared: false,
  });
  const m = card ? patch : create;
  const errors = fieldErrors(m.error);
  const general = m.error && !Object.keys(errors).length ? messageFor(m.error) : null;

  const submit = (e: FormEvent) => {
    e.preventDefault();
    const body = { componentId: f.componentId, front: f.front.trim(), back: f.back.trim() };
    if (card) {
      patch.mutate(
        { card, body },
        {
          onSuccess: onClose,
          onError: (err) => {
            if (conflictCurrent(err))
              toast({ message: "This card changed elsewhere; reopen it to see the new text.", tone: "danger" });
          },
        },
      );
    } else {
      create.mutate(
        { ...body, shared: editor && f.shared, source: "Manual" },
        {
          onSuccess: (c) => {
            toast({ message: `Card ${c.code} added` });
            onClose();
          },
        },
      );
    }
  };

  return (
    <Dialog
      open
      onOpenChange={(o) => !o && onClose()}
      title={card ? `Card ${card.code}` : "New flashcard"}
      className="w-[min(640px,calc(100vw-32px))]"
    >
      <form className="flex flex-col gap-4" onSubmit={submit} noValidate>
        {general ? <Banner tone="danger">{general}</Banner> : null}
        <Field label="Lecture" error={errors.componentId}>
          {(ids) => (
            <Select {...ids} value={f.componentId} onChange={(e) => setF({ ...f, componentId: e.currentTarget.value })}>
              <option value="">Pick a lecture</option>
              {(lectures.data ?? []).map((l) => (
                <option key={l.id} value={l.id}>
                  {l.code} {l.name}
                </option>
              ))}
            </Select>
          )}
        </Field>
        <Field label="Front (markdown, $maths$)" error={errors.front}>
          {(ids) => (
            <Textarea
              {...ids}
              autoFocus={!draft?.front}
              value={f.front}
              onChange={(e) => setF({ ...f, front: e.currentTarget.value })}
            />
          )}
        </Field>
        <Field label="Back" error={errors.back}>
          {(ids) => <Textarea {...ids} value={f.back} onChange={(e) => setF({ ...f, back: e.currentTarget.value })} />}
        </Field>
        {f.front.trim() || f.back.trim() ? (
          <div className="flex flex-col gap-2 rounded-md border border-border p-3">
            <Markdown>{f.front || "_Front_"}</Markdown>
            <div className="border-t border-border pt-2">
              <Markdown>{f.back || "_Back_"}</Markdown>
            </div>
          </div>
        ) : null}
        {!card && editor ? (
          <label className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              className="size-4 accent-[var(--primary)]"
              checked={f.shared}
              onChange={(e) => setF({ ...f, shared: e.currentTarget.checked })}
            />
            A course card: everyone on the course studies it
          </label>
        ) : null}
        <div className="flex justify-end gap-2">
          <Button onClick={onClose}>Cancel</Button>
          <Button
            type="submit"
            variant="primary"
            disabled={m.isPending || !f.componentId || !f.front.trim() || !f.back.trim()}
          >
            {card ? "Save" : "Add card"}
          </Button>
        </div>
      </form>
    </Dialog>
  );
}

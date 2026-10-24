import { useState } from "react";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/field";
import { Dialog } from "@/components/ui/overlays";
import type { Deadline } from "@/features/courses/api";
import { useSetLayout } from "@/features/deadlines/api";
import { useLectures } from "@/features/lectures/api";
import { messageFor } from "@/lib/errors";

type Row = { questions: string; open: string };

/** The exam's layout: questions per lecture and how many are open (the mock builder uses it). */
export function LayoutDialog({
  deadline,
  open,
  onOpenChange,
}: {
  deadline: Deadline;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const lectures = useLectures(deadline.courseId);
  const save = useSetLayout();
  const [rows, setRows] = useState<Record<string, Row>>(() =>
    Object.fromEntries(
      deadline.layout.map((l) => [
        l.componentId,
        { questions: l.questionCount ? String(l.questionCount) : "", open: l.openCount ? String(l.openCount) : "" },
      ]),
    ),
  );
  const num = (s: string | undefined) => (s && Number(s) > 0 ? Number(s) : null);
  const total = Object.values(rows).reduce((n, r) => n + (num(r.questions) ?? 0), 0);
  const set = (id: string, k: keyof Row, v: string) =>
    setRows((r) => ({ ...r, [id]: { questions: "", open: "", ...r[id], [k]: v.replace(/\D/g, "").slice(0, 3) } }));

  const submit = () => {
    const components = Object.entries(rows)
      .filter(([, r]) => num(r.questions) || num(r.open))
      .map(([componentId, r]) => ({ componentId, questionCount: num(r.questions), openCount: num(r.open) }));
    save.mutate({ id: deadline.id, components }, { onSuccess: () => onOpenChange(false) });
  };

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title="Exam layout"
      description="How many questions each lecture gets, and how many of them are open. Mocks follow this layout."
      className="w-[min(560px,calc(100vw-32px))]"
      footer={
        <>
          <Button onClick={() => onOpenChange(false)}>Cancel</Button>
          <Button variant="primary" disabled={save.isPending} onClick={submit}>
            Save layout
          </Button>
        </>
      }
    >
      {save.error ? <Banner tone="danger">{messageFor(save.error)}</Banner> : null}
      {!lectures.data?.length ? (
        <p className="m-0 text-muted">Add lectures first; the layout lists them.</p>
      ) : (
        <table className="w-full border-collapse text-sm">
          <thead>
            <tr className="text-left text-xs uppercase tracking-[0.06em] text-muted">
              <th className="pb-2 font-medium">Lecture</th>
              <th className="w-24 pb-2 font-medium">Questions</th>
              <th className="w-24 pb-2 font-medium">Open</th>
            </tr>
          </thead>
          <tbody>
            {lectures.data.map((l) => (
              <tr key={l.id} className="border-t border-border">
                <td className="py-1.5 pr-2">
                  <span className="font-mono text-xs text-muted">{l.code}</span> {l.name}
                </td>
                <td className="py-1.5 pr-2">
                  <Input
                    aria-label={`Questions from ${l.code}`}
                    inputMode="numeric"
                    className="h-8 font-mono tnum"
                    value={rows[l.id]?.questions ?? ""}
                    onChange={(e) => set(l.id, "questions", e.currentTarget.value)}
                  />
                </td>
                <td className="py-1.5">
                  <Input
                    aria-label={`Open questions from ${l.code}`}
                    inputMode="numeric"
                    className="h-8 font-mono tnum"
                    value={rows[l.id]?.open ?? ""}
                    onChange={(e) => set(l.id, "open", e.currentTarget.value)}
                  />
                </td>
              </tr>
            ))}
          </tbody>
          <tfoot>
            <tr className="border-t border-border font-medium">
              <td className="pt-2">Total</td>
              <td className="pt-2 font-mono tnum">{total}</td>
              <td />
            </tr>
          </tfoot>
        </table>
      )}
    </Dialog>
  );
}

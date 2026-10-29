import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Sparkles } from "lucide-react";
import { useEffect, useState } from "react";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Field, Input, Select } from "@/components/ui/field";
import { Dialog, Tooltip } from "@/components/ui/overlays";
import { useToast } from "@/components/ui/toast";
import { keys as bankKeys } from "@/features/bank/api";
import { useLectures } from "@/features/lectures/api";
import { api, unwrap } from "@/lib/api";
import { fieldErrors, messageFor } from "@/lib/errors";
import { useAi, useAiSettings } from "./api";
import { reasonText } from "./lib/format";

// "Generate questions" in the question bank (frontend-spec 4, AI gate): pick a lecture and how many
// (up to 20); the job runs in the background, and the bank refreshes when it's done. Generated
// questions are marked, and wait as drafts unless you auto-approve them in Settings > AI.

function useJob(id: string | null) {
  return useQuery({
    queryKey: ["ai", "generate", id],
    enabled: !!id,
    queryFn: () => unwrap(api.GET("/ai/generate/{job_id}", { params: { path: { job_id: id! } } })),
    refetchInterval: (q) => (q.state.data && !["Queued", "Running"].includes(q.state.data.status) ? false : 2000),
  });
}

export function GenerateButton({ courseId, componentId }: { courseId: string; componentId?: string | undefined }) {
  const gate = useAi("Generation", courseId);
  const [open, setOpen] = useState(false);
  const [job, setJob] = useState<string | null>(null);
  const status = useJob(job);
  const qc = useQueryClient();
  const toast = useToast();
  const settings = useAiSettings();

  // Tell the user once the job settles, and show the new questions.
  useEffect(() => {
    const j = status.data;
    if (!j || ["Queued", "Running"].includes(j.status)) return;
    setJob(null);
    void qc.invalidateQueries({ queryKey: bankKeys.bank(courseId) });
    if (j.status === "Done") {
      const drafts = settings.data && !settings.data.autoApproveGenerated;
      toast({
        message: j.createdCount
          ? `${j.createdCount} ${j.createdCount === 1 ? "question" : "questions"} added${drafts ? " as drafts" : ""}`
          : "No new questions passed the checks. Try again or pick another lecture.",
      });
    } else {
      toast({ message: "Generating questions failed. Try again.", tone: "danger" });
    }
  }, [status.data, qc, courseId, toast, settings.data]);

  if (!gate.visible) return null;
  const why = gate.allowed ? null : reasonText(gate.reason, courseId);
  const busy = !!job;
  const button = (
    <Button size="sm" disabled={!gate.allowed || busy} onClick={() => setOpen(true)}>
      <Sparkles className="size-4" aria-hidden="true" />
      {busy ? "Generating…" : "Generate questions"}
    </Button>
  );
  return (
    <>
      {why ? (
        <Tooltip label={why.text}>
          <span tabIndex={0}>{button}</span>
        </Tooltip>
      ) : (
        button
      )}
      {open ? (
        <GenerateDialog
          courseId={courseId}
          componentId={componentId}
          onClose={() => setOpen(false)}
          onStarted={(id) => {
            setJob(id);
            setOpen(false);
          }}
        />
      ) : null}
    </>
  );
}

function GenerateDialog({
  courseId,
  componentId,
  onClose,
  onStarted,
}: {
  courseId: string;
  componentId?: string | undefined;
  onClose: () => void;
  onStarted: (jobId: string) => void;
}) {
  const lectures = useLectures(courseId);
  const [lecture, setLecture] = useState(componentId ?? "");
  const [n, setN] = useState("5");
  const start = useMutation({
    mutationFn: () => unwrap(api.POST("/ai/generate", { body: { courseId, componentId: lecture, n: Number(n) || 5 } })),
    onSuccess: (j) => onStarted(j.id),
  });
  const errors = fieldErrors(start.error);
  const count = Number(n);
  const bad = !Number.isInteger(count) || count < 1 || count > 20;
  return (
    <Dialog
      open
      onOpenChange={(o) => !o && onClose()}
      title="Generate questions"
      description="AI writes new practice questions in your exam's style, next to the ones you have. They're marked Generated."
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={!lecture || bad || start.isPending} onClick={() => start.mutate()}>
            Generate
          </Button>
        </>
      }
    >
      {start.error && !Object.keys(errors).length ? <Banner tone="danger">{messageFor(start.error)}</Banner> : null}
      <Field label="Lecture" error={errors.componentId}>
        {(ids) => (
          <Select {...ids} value={lecture} onChange={(e) => setLecture(e.currentTarget.value)}>
            <option value="">Pick a lecture</option>
            {(lectures.data ?? []).map((l) => (
              <option key={l.id} value={l.id}>
                {l.code} {l.name}
              </option>
            ))}
          </Select>
        )}
      </Field>
      <Field label="How many" error={errors.n ?? (bad ? "1 to 20" : undefined)} hint="1 to 20">
        {(ids) => (
          <Input {...ids} type="number" min={1} max={20} value={n} onChange={(e) => setN(e.currentTarget.value)} />
        )}
      </Field>
    </Dialog>
  );
}

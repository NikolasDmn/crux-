import { Sparkles } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Dialog, Tooltip } from "@/components/ui/overlays";
import { AiCourseSetup } from "./ai-course-setup";
import { useAi } from "./api";
import { reasonText } from "./lib/format";

// "Import with AI" on a course's Lectures (frontend-spec 9, journey 21): more files into a course you
// edit. Lectures it already has show as matches in the review and link up on apply.
export function CourseImportButton({ courseId }: { courseId: string }) {
  const gate = useAi("CourseImport", courseId);
  const [open, setOpen] = useState(false);
  if (!gate.visible) return null;
  const why = gate.allowed ? null : reasonText(gate.reason, courseId);
  const button = (
    <Button size="sm" disabled={!gate.allowed} onClick={() => setOpen(true)}>
      <Sparkles className="size-4" aria-hidden="true" />
      Import with AI
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
      <Dialog open={open} onOpenChange={setOpen} title="Import with AI" className="w-[min(640px,calc(100vw-32px))]">
        <AiCourseSetup courseId={courseId} />
      </Dialog>
    </>
  );
}

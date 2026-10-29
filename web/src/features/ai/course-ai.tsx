import { Card, SectionLabel } from "@/components/ui/card";
import { Check, Field, Select } from "@/components/ui/field";
import { Skeleton } from "@/components/ui/misc";
import { useToast } from "@/components/ui/toast";
import { messageFor } from "@/lib/errors";
import { useAiSettings, useCourseAi, usePutCourseAi, useTutors, type CourseAi } from "./api";

// Course settings, "AI for this course" (frontend-spec 4): on/off, tutor and nightly generation.
// These are yours, not the course's: each member decides for themselves. Shown only with AI on.

export function CourseAiSettings({ courseId }: { courseId: string }) {
  const user = useAiSettings();
  const on = user.data?.enabled === true;
  const course = useCourseAi(courseId, on);
  const tutors = useTutors(on);
  const put = usePutCourseAi(courseId);
  const toast = useToast();
  if (!on) return null;
  if (!course.data) return <Skeleton className="h-32" />;
  const c = course.data;
  // PUT replaces the row: always send every field.
  const save = (p: Partial<Pick<CourseAi, "aiEnabled" | "tutorId" | "nightlyGeneration">>) =>
    put.mutate(
      {
        aiEnabled: c.aiEnabled,
        tutorId: c.tutorId,
        nightlyGeneration: c.nightlyGeneration,
        generationPolicy: c.generationPolicy,
        ...p,
      },
      {
        onSuccess: () => toast({ message: "Saved" }),
        onError: (e) => toast({ message: messageFor(e), tone: "danger" }),
      },
    );
  const enabled = c.aiEnabled !== false;
  return (
    <Card className="flex flex-col gap-4">
      <SectionLabel>AI for this course</SectionLabel>
      <p className="m-0 text-sm text-muted">These settings are yours; other members choose their own.</p>
      <Check
        checked={enabled}
        disabled={put.isPending}
        onChange={(v) => save({ aiEnabled: v ? null : false })}
        label="Use AI in this course"
      />
      {enabled ? (
        <>
          <Field label="Tutor" hint="Overrides your default tutor here.">
            {(ids) => (
              <Select
                {...ids}
                className="sm:max-w-80"
                value={c.tutorId ?? ""}
                disabled={put.isPending}
                onChange={(e) => save({ tutorId: e.currentTarget.value || null })}
              >
                <option value="">Your default</option>
                {(tutors.data ?? []).map((t) => (
                  <option key={t.id} value={t.id}>
                    {t.name}
                  </option>
                ))}
              </Select>
            )}
          </Field>
          <Field label="Nightly practice questions">
            {(ids) => (
              <Select
                {...ids}
                className="sm:max-w-80"
                value={c.nightlyGeneration === null ? "" : String(c.nightlyGeneration)}
                disabled={put.isPending}
                onChange={(e) =>
                  save({ nightlyGeneration: e.currentTarget.value === "" ? null : e.currentTarget.value === "true" })
                }
              >
                <option value="">As in your AI settings ({user.data?.nightlyGeneration ? "on" : "off"})</option>
                <option value="true">On for this course</option>
                <option value="false">Off for this course</option>
              </Select>
            )}
          </Field>
        </>
      ) : null}
    </Card>
  );
}

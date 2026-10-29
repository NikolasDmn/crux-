import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useState, type FormEvent } from "react";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Field, Input, Select } from "@/components/ui/field";
import { idempotencyKey, useCreateCourse, useShownGradeScales } from "@/features/courses/api";
import { ColourPicker } from "@/features/courses/components/colour-picker";
import { normalizeCode } from "@/features/courses/lib/roles";
import { DEFAULT_COURSE_COLOR } from "@/lib/color";
import { Segmented } from "@/components/ui/segmented";
import { AiCourseSetup } from "@/features/ai/ai-course-setup";
import { useAi } from "@/features/ai/api";
import { reasonText } from "@/features/ai/lib/format";
import { fieldErrors, messageFor } from "@/lib/errors";

// New course (frontend-spec 4): Manual, or "Set up with AI" (only with the CourseImport gate open).
export const Route = createFileRoute("/_app/courses/new")({ component: NewCourse });

function NewCourse() {
  const navigate = useNavigate();
  const scales = useShownGradeScales();
  const create = useCreateCourse();
  const [key] = useState(idempotencyKey);
  const gate = useAi("CourseImport");
  const [path, setPath] = useState<"manual" | "ai">("manual");
  const [f, setF] = useState({
    name: "",
    code: "",
    institution: "",
    term: "",
    color: DEFAULT_COURSE_COLOR.hex,
    gradeScaleId: "",
  });
  const errors = fieldErrors(create.error);
  const general = create.error && !Object.keys(errors).length ? messageFor(create.error) : null;

  const submit = (e: FormEvent) => {
    e.preventDefault();
    create.mutate(
      {
        key,
        fields: {
          name: f.name.trim(),
          code: f.code,
          institution: f.institution.trim() || null,
          term: f.term.trim() || null,
          color: f.color,
          gradeScaleId: f.gradeScaleId || null,
        },
      },
      { onSuccess: (c) => void navigate({ to: "/c/$courseId", params: { courseId: c.id } }) },
    );
  };

  return (
    <div className="flex max-w-[640px] flex-col gap-6">
      <h1 className="m-0 text-xl font-semibold tracking-[-0.01em]">New course</h1>
      {gate.visible ? (
        <div className="flex flex-col gap-1.5">
          <Segmented
            label="How to start"
            value={path}
            onChange={(v) => (v === "manual" || gate.allowed) && setPath(v)}
            options={[
              { value: "manual", label: "Manual" },
              { value: "ai", label: "Set up with AI" },
            ]}
          />
          {!gate.allowed ? (
            <span className="text-sm text-muted">{reasonText(gate.reason)?.text ?? "AI setup isn't available."}</span>
          ) : null}
        </div>
      ) : null}
      {path === "ai" && gate.allowed ? (
        <Card>
          <AiCourseSetup />
        </Card>
      ) : (
        <Card>
          <form className="flex flex-col gap-4" onSubmit={submit} noValidate>
            {general ? <Banner tone="danger">{general}</Banner> : null}
            <Field label="Name" error={errors.name}>
              {(ids) => (
                <Input
                  {...ids}
                  required
                  autoFocus
                  placeholder="Business statistics"
                  value={f.name}
                  onChange={(e) => setF({ ...f, name: e.currentTarget.value })}
                />
              )}
            </Field>
            <div className="grid gap-4 sm:grid-cols-2">
              <Field label="Code" error={errors.code} hint="Short, used on cards: BSA017.">
                {(ids) => (
                  <Input
                    {...ids}
                    required
                    placeholder="BSA"
                    className="font-mono uppercase"
                    value={f.code}
                    onChange={(e) => setF({ ...f, code: normalizeCode(e.currentTarget.value) })}
                  />
                )}
              </Field>
              <Field label="Grade scale" error={errors.gradeScaleId}>
                {(ids) => (
                  <Select
                    {...ids}
                    value={f.gradeScaleId}
                    onChange={(e) => setF({ ...f, gradeScaleId: e.currentTarget.value })}
                  >
                    <option value="">Percent (default)</option>
                    {(scales.data ?? [])
                      .filter((s) => s.name !== "Percent")
                      .map((s) => (
                        <option key={s.id} value={s.id}>
                          {s.name}
                        </option>
                      ))}
                  </Select>
                )}
              </Field>
              <Field label="Institution" error={errors.institution}>
                {(ids) => (
                  <Input
                    {...ids}
                    placeholder="Optional"
                    value={f.institution}
                    onChange={(e) => setF({ ...f, institution: e.currentTarget.value })}
                  />
                )}
              </Field>
              <Field label="Term" error={errors.term}>
                {(ids) => (
                  <Input
                    {...ids}
                    placeholder="2026 Q1"
                    value={f.term}
                    onChange={(e) => setF({ ...f, term: e.currentTarget.value })}
                  />
                )}
              </Field>
            </div>
            <div className="flex flex-col gap-2">
              <span className="text-sm font-medium">Colour</span>
              <ColourPicker value={f.color} onChange={(color) => setF({ ...f, color })} />
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <Button onClick={() => window.history.back()}>Cancel</Button>
              <Button type="submit" variant="primary" disabled={create.isPending || !f.name.trim() || !f.code}>
                Create course
              </Button>
            </div>
          </form>
        </Card>
      )}
    </div>
  );
}

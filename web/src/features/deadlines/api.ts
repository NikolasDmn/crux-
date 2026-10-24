import { useMutation, useQueryClient } from "@tanstack/react-query";
import { keys as courseKeys, type Deadline } from "@/features/courses/api";
import { invalidatePlan } from "@/features/plan/api";
import { api, ifMatch, unwrap, type Schemas } from "@/lib/api";

export type DeadlineInput = Schemas["DeadlineInput"];
export type LayoutItem = Schemas["LayoutInputItem"];

// Any deadline change rebuilds the plan on the server (spec 5.8); a result can archive the course.
function after(qc: ReturnType<typeof useQueryClient>, d: Deadline) {
  void qc.invalidateQueries({ queryKey: courseKeys.deadlines(d.courseId) });
  void qc.invalidateQueries({ queryKey: courseKeys.courses });
  void qc.invalidateQueries({ queryKey: courseKeys.course(d.courseId) });
  void qc.invalidateQueries({ queryKey: ["stats", d.courseId] });
  invalidatePlan(qc);
}

export function useCreateDeadline(courseId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: DeadlineInput) =>
      unwrap(api.POST("/courses/{id}/deadlines", { params: { path: { id: courseId } }, body })),
    onSuccess: (d) => after(qc, d),
  });
}

export function usePatchDeadline() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, version, body }: { id: string; version: number; body: DeadlineInput }) =>
      unwrap(api.PATCH("/deadlines/{id}", { params: { path: { id }, header: ifMatch(version) }, body })),
    onSuccess: (d) => after(qc, d),
  });
}

export function useSetLayout() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, components }: { id: string; components: LayoutItem[] }) =>
      unwrap(api.PUT("/deadlines/{id}/layout", { params: { path: { id } }, body: { components } })),
    onSuccess: (d) => after(qc, d),
  });
}

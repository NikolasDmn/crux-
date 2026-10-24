import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, ifMatch, unwrap, type Schemas } from "@/lib/api";

export type PlanEntry = Schemas["PlanEntryView"];
export type EntryInput = Schemas["EntryInput"];
export type EntryPatch = Schemas["EntryPatch"];
export type Short = Schemas["ShortView"];

export const keys = {
  all: ["plan"] as const,
  range: (from: string, to: string, courseId?: string) => ["plan", from, to, courseId ?? "all"] as const,
  short: ["plan", "short"] as const,
  settings: ["me", "settings"] as const,
};

/** Everything that reads the plan: the calendar, Home's Today, /next and the week view. */
export function invalidatePlan(qc: ReturnType<typeof useQueryClient>) {
  void qc.invalidateQueries({ queryKey: keys.all });
  void qc.invalidateQueries({ queryKey: ["next"] });
  void qc.invalidateQueries({ queryKey: ["week"] });
}

export function usePlan(from: Date, to: Date, courseId?: string) {
  const f = from.toISOString();
  const t = to.toISOString();
  return useQuery({
    queryKey: keys.range(f, t, courseId),
    queryFn: () =>
      unwrap(api.GET("/plan", { params: { query: { from: f, to: t, ...(courseId ? { courseId } : {}) } } })),
    staleTime: 30_000,
  });
}

/** "X h short" (courses whose lectures don't fit before the final week). */
export function useShort() {
  return useQuery({ queryKey: keys.short, queryFn: () => unwrap(api.GET("/plan/short")), staleTime: 60_000 });
}

/** `/me/settings`: the study window lives in `extra.studyStart` / `studyEnd`. */
export function useSettings() {
  return useQuery({ queryKey: keys.settings, queryFn: () => unwrap(api.GET("/me/settings")), staleTime: 5 * 60_000 });
}

export function useCreateEntry() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: EntryInput) => unwrap(api.POST("/plan/entries", { body })),
    onSuccess: () => invalidatePlan(qc),
  });
}

/** A new time moves a Planned entry (the old one becomes Moved) and pins the new one. */
export function usePatchEntry() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, version, patch }: { id: string; version: number; patch: EntryPatch }) =>
      unwrap(api.PATCH("/plan/entries/{id}", { params: { path: { id }, header: ifMatch(version) }, body: patch })),
    onSuccess: () => invalidatePlan(qc),
  });
}

/** Your own entries are deleted; a planner entry is skipped and pinned. */
export function useDeleteEntry() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => unwrap(api.DELETE("/plan/entries/{id}", { params: { path: { id } } })),
    onSuccess: () => invalidatePlan(qc),
  });
}

export function useRebuild() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => unwrap(api.POST("/plan/rebuild")),
    onSuccess: (r) => {
      qc.setQueryData(keys.short, r.short);
      invalidatePlan(qc);
    },
  });
}

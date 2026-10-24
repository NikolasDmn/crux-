import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, unwrap, type Schemas } from "@/lib/api";

export type NextView = Schemas["NextView"];
export type Step = Schemas["Step"];
export type WarmupItem = Schemas["WarmupItem"];
export type AttemptInput = Schemas["AttemptInput"];
export type CourseSummary = Schemas["CourseSummary"];
export type Pattern = Schemas["Pattern"];
export type WeekView = Schemas["WeekView"];

export const keys = {
  next: ["next"] as const,
  warmup: (courseId: string) => ["next", "warmup", courseId] as const,
  summary: (courseId: string) => ["stats", courseId, "summary"] as const,
  patterns: ["insight", "patterns"] as const,
  week: (isoWeek: string) => ["week", isoWeek] as const,
};

export function useNext() {
  return useQuery({ queryKey: keys.next, queryFn: () => unwrap(api.GET("/next")), staleTime: 30_000 });
}

/** Fetched once per run: the server builds it fresh each call (spec 5.9). */
export function useWarmup(courseId: string | undefined) {
  return useQuery({
    queryKey: keys.warmup(courseId ?? ""),
    enabled: !!courseId,
    queryFn: () => unwrap(api.GET("/courses/{id}/warmup", { params: { path: { id: courseId! } } })),
    staleTime: Infinity,
  });
}

export function useAdvance() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: Schemas["AdvanceInput"]) => unwrap(api.POST("/next/advance", { body })),
    onSuccess: (_, body) => {
      if (body.planEntryId) {
        void qc.invalidateQueries({ queryKey: ["plan"] });
        void qc.invalidateQueries({ queryKey: ["week"] });
      }
      // Stale, not refetched: the run keeps the steps it started with until "What's next?".
      void qc.invalidateQueries({ queryKey: keys.next, refetchType: "none" });
    },
  });
}

export function useAttempt() {
  return useMutation({ mutationFn: (body: AttemptInput) => unwrap(api.POST("/attempts", { body })) });
}

export function useSummary(courseId: string, enabled = true) {
  return useQuery({
    queryKey: keys.summary(courseId),
    enabled,
    queryFn: () => unwrap(api.GET("/courses/{id}/stats/summary", { params: { path: { id: courseId } } })),
    staleTime: 60_000,
  });
}

export function usePatterns() {
  return useQuery({
    queryKey: keys.patterns,
    queryFn: () => unwrap(api.GET("/insight/patterns")),
    staleTime: 5 * 60_000,
  });
}

export function useWeek(isoWeek: string) {
  return useQuery({
    queryKey: keys.week(isoWeek),
    queryFn: () => unwrap(api.GET("/week", { params: { query: { isoWeek } } })),
    staleTime: 60_000,
  });
}

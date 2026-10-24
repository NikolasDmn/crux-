import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, unwrap, type Schemas } from "@/lib/api";

export type MaterialCheck = Schemas["MaterialCheck"];

export const keys = {
  checks: (materialId: string) => ["checks", materialId] as const,
};

/** A material's check questions, in reading order (no keys). */
export function useChecks(materialId: string | undefined) {
  return useQuery({
    queryKey: keys.checks(materialId ?? ""),
    enabled: !!materialId,
    queryFn: () => unwrap(api.GET("/materials/{id}/checks", { params: { path: { id: materialId! } } })),
  });
}

/** Replace a material's checks (Owner or Editor). */
export function useSetChecks(materialId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (checks: { questionId: string; position: number }[]) =>
      unwrap(api.PUT("/materials/{id}/checks", { params: { path: { id: materialId } }, body: { checks } })),
    onSuccess: (c) => qc.setQueryData(keys.checks(materialId), c),
  });
}

/** "Quiz me": a few questions on a lecture or concept, never-tried first; `exclude` skips shown ones. */
export function fetchQuiz(
  courseId: string,
  q: { componentId?: string | undefined; conceptId?: string | undefined; n?: number; exclude?: string[] },
) {
  return unwrap(
    api.GET("/courses/{id}/quiz", {
      params: {
        path: { id: courseId },
        query: {
          ...(q.componentId ? { componentId: q.componentId } : {}),
          ...(q.conceptId ? { conceptId: q.conceptId } : {}),
          n: q.n ?? 2,
          ...(q.exclude?.length ? { exclude: q.exclude.join(",") } : {}),
        },
      },
    }),
  );
}

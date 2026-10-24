import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, ifMatch, unwrap, type Schemas } from "@/lib/api";

export type Lecture = Schemas["ComponentView"];
export type LectureFields = Schemas["ComponentFields"];
export type Progress = Schemas["LectureProgress"];
export type Material = Schemas["MaterialView"];
export type MaterialFields = Schemas["MaterialFields"];

export const keys = {
  lectures: (courseId: string) => ["course", courseId, "lectures"] as const,
  progress: (courseId: string) => ["course", courseId, "progress"] as const,
  materials: (componentId: string) => ["lecture", componentId, "materials"] as const,
};

export function useLectures(courseId: string) {
  return useQuery({
    queryKey: keys.lectures(courseId),
    queryFn: async () =>
      (await unwrap(api.GET("/courses/{id}/components", { params: { path: { id: courseId } } }))).sort(
        (a, b) => a.sortOrder - b.sortOrder,
      ),
  });
}

export function useProgress(courseId: string) {
  return useQuery({
    queryKey: keys.progress(courseId),
    queryFn: () => unwrap(api.GET("/courses/{id}/progress", { params: { path: { id: courseId } } })),
  });
}

export function useSetProgress(courseId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: Schemas["ProgressPatch"]) =>
      unwrap(api.PATCH("/courses/{id}/progress", { params: { path: { id: courseId } }, body })),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: keys.progress(courseId) });
      // Marking a lecture done rebuilds the plan (B5-7).
      void qc.invalidateQueries({ queryKey: ["plan"] });
    },
  });
}

export function useCreateLecture(courseId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ fields, key }: { fields: LectureFields; key: string }) =>
      unwrap(
        api.POST("/courses/{id}/components", {
          params: { path: { id: courseId }, header: { "Idempotency-Key": key } },
          body: fields,
        }),
      ),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: keys.lectures(courseId) });
      void qc.invalidateQueries({ queryKey: keys.progress(courseId) });
    },
  });
}

export function usePatchLecture(courseId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, fields, version }: { id: string; fields: LectureFields; version: number }) =>
      unwrap(api.PATCH("/components/{id}", { params: { path: { id }, header: ifMatch(version) }, body: fields })),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: keys.lectures(courseId) });
      void qc.invalidateQueries({ queryKey: keys.progress(courseId) });
    },
  });
}

export function useDeleteLecture(courseId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => unwrap(api.DELETE("/components/{id}", { params: { path: { id } } })),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: keys.lectures(courseId) });
      void qc.invalidateQueries({ queryKey: keys.progress(courseId) });
    },
  });
}

export function useMaterials(componentId: string) {
  return useQuery({
    queryKey: keys.materials(componentId),
    queryFn: async () =>
      (await unwrap(api.GET("/components/{id}/materials", { params: { path: { id: componentId } } }))).sort(
        (a, b) => a.sortOrder - b.sortOrder,
      ),
  });
}

/** A material by id: there is no single-material route, so it is found among the course's lectures. */
export function useMaterial(courseId: string, materialId: string) {
  const qc = useQueryClient();
  return useQuery({
    queryKey: ["material", materialId],
    queryFn: async () => {
      const lectures = await qc.fetchQuery({
        queryKey: keys.lectures(courseId),
        queryFn: () => unwrap(api.GET("/courses/{id}/components", { params: { path: { id: courseId } } })),
      });
      for (const l of lectures) {
        const list = await qc.fetchQuery({
          queryKey: keys.materials(l.id),
          queryFn: () => unwrap(api.GET("/components/{id}/materials", { params: { path: { id: l.id } } })),
        });
        const m = list.find((x) => x.id === materialId);
        if (m) return { material: m, lecture: l };
      }
      throw Object.assign(new Error("not found"), { status: 404 });
    },
  });
}

export function useCreateMaterial(componentId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ fields, key }: { fields: MaterialFields; key: string }) =>
      unwrap(
        api.POST("/components/{id}/materials", {
          params: { path: { id: componentId }, header: { "Idempotency-Key": key } },
          body: fields,
        }),
      ),
    onSuccess: () => void qc.invalidateQueries({ queryKey: keys.materials(componentId) }),
  });
}

export function usePatchMaterial(componentId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, fields, version }: { id: string; fields: MaterialFields; version: number }) =>
      unwrap(api.PATCH("/materials/{id}", { params: { path: { id }, header: ifMatch(version) }, body: fields })),
    onSuccess: (m) => {
      void qc.invalidateQueries({ queryKey: keys.materials(componentId) });
      void qc.invalidateQueries({ queryKey: ["material", m.id] });
    },
  });
}

export function useDeleteMaterial(componentId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => unwrap(api.DELETE("/materials/{id}", { params: { path: { id } } })),
    onSuccess: () => void qc.invalidateQueries({ queryKey: keys.materials(componentId) }),
  });
}

/** Lectures in a new order: only the ones whose position changed are patched. */
export function reorder<T extends { id: string; sortOrder: number }>(
  list: T[],
  from: number,
  to: number,
): { id: string; sortOrder: number }[] {
  if (from === to || from < 0 || to < 0 || from >= list.length || to >= list.length) return [];
  const next = [...list];
  const [moved] = next.splice(from, 1);
  next.splice(to, 0, moved!);
  return next
    .map((x, i) => ({ id: x.id, sortOrder: (i + 1) * 10, was: x.sortOrder }))
    .filter((x) => x.sortOrder !== x.was)
    .map(({ id, sortOrder }) => ({ id, sortOrder }));
}

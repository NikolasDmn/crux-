import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, ifMatch, unwrap, type Schemas } from "@/lib/api";

export type Slip = Schemas["SlipView"];
export type SlipInput = Schemas["SlipInput"];
export type WeakSpot = Schemas["WeakSpotView"];
export type WeakSpots = Schemas["WeakSpotsView"];
export type Trap = Schemas["TrapView"];

export const SLIP_TYPES = ["Concept", "Formula", "Unit", "Arithmetic", "Sign", "Misread", "OptionMatching"] as const;
export const SEVERITIES = ["Low", "Medium", "High", "Critical"] as const;

export const keys = {
  slips: (q: { courseId?: string | undefined; status?: string | undefined }) => ["slips", q] as const,
  spots: (courseId: string) => ["weak-spots", courseId] as const,
  traps: (courseId: string) => ["traps", courseId] as const,
};

/** Slips and weak spots feed /next, warm-ups, stats and the patterns: refresh them together. */
function invalidate(qc: ReturnType<typeof useQueryClient>, courseId?: string) {
  void qc.invalidateQueries({ queryKey: ["slips"] });
  void qc.invalidateQueries({ queryKey: ["weak-spots"] });
  void qc.invalidateQueries({ queryKey: ["insight"] });
  void qc.invalidateQueries({ queryKey: ["next"] });
  if (courseId) void qc.invalidateQueries({ queryKey: ["stats", courseId] });
}

export function useSlips(q: { courseId?: string | undefined; status?: string | undefined; type?: string | undefined }) {
  return useQuery({
    queryKey: keys.slips(q),
    queryFn: () =>
      unwrap(
        api.GET("/slips", {
          params: {
            query: {
              ...(q.courseId ? { courseId: q.courseId } : {}),
              ...(q.status ? { status: q.status } : {}),
              ...(q.type ? { type: q.type } : {}),
            },
          },
        }),
      ),
  });
}

export function useLogSlip() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: SlipInput) => unwrap(api.POST("/slips", { body })),
    onSuccess: (s) => {
      invalidate(qc, s.courseId);
      if (s.mockId) void qc.invalidateQueries({ queryKey: ["mock", s.mockId] });
    },
  });
}

export function usePatchSlip() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, body }: { id: string; body: Schemas["SlipPatch"] }) =>
      unwrap(api.PATCH("/slips/{id}", { params: { path: { id } }, body })),
    onSuccess: (s) => invalidate(qc, s.courseId),
  });
}

export function useWeakSpots(courseId: string, enabled = true) {
  return useQuery({
    queryKey: keys.spots(courseId),
    enabled,
    queryFn: () => unwrap(api.GET("/courses/{id}/weak-spots", { params: { path: { id: courseId } } })),
  });
}

export function useCreateWeakSpot(courseId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: Schemas["WeakSpotInput"]) =>
      unwrap(api.POST("/courses/{id}/weak-spots", { params: { path: { id: courseId } }, body })),
    onSuccess: () => invalidate(qc, courseId),
  });
}

export function usePatchWeakSpot(courseId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ spot, body }: { spot: WeakSpot; body: Schemas["WeakSpotPatch"] }) =>
      unwrap(api.PATCH("/weak-spots/{id}", { params: { path: { id: spot.id }, header: ifMatch(spot.version) }, body })),
    onSuccess: () => invalidate(qc, courseId),
  });
}

export function useTraps(courseId: string, enabled = true) {
  return useQuery({
    queryKey: keys.traps(courseId),
    enabled,
    queryFn: () => unwrap(api.GET("/courses/{id}/traps", { params: { path: { id: courseId } } })),
  });
}

export function useTrapActions(courseId: string) {
  const qc = useQueryClient();
  const done = () => void qc.invalidateQueries({ queryKey: keys.traps(courseId) });
  const add = useMutation({
    mutationFn: (body: Schemas["TrapInput"]) =>
      unwrap(api.POST("/courses/{id}/traps", { params: { path: { id: courseId } }, body })),
    onSuccess: done,
  });
  const remove = useMutation({
    mutationFn: (trapId: string) =>
      unwrap(api.DELETE("/courses/{id}/traps/{trapId}", { params: { path: { id: courseId, trapId } } })),
    onSuccess: done,
  });
  return { add, remove };
}

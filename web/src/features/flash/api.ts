import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, ifMatch, unwrap, type Schemas } from "@/lib/api";

export type Card = Schemas["CardView"];
export type Queue = Schemas["QueueView"];
export type NextDue = Schemas["NextDue"];

export const keys = {
  queue: (courseId?: string) => ["flash", "queue", courseId ?? "all"] as const,
  cards: (courseId: string) => ["flash", "cards", courseId] as const,
};

/** Cards due now, across courses (sidebar count). */
export function useDueCount() {
  return useQuery({
    queryKey: keys.queue(),
    queryFn: async () => (await unwrap(api.GET("/flash/queue"))).dueCount,
    staleTime: 60_000,
  });
}

/** Today's queue: due cards then new ones, with each card's next interval per rating. */
export function useQueue(courseId?: string, enabled = true) {
  return useQuery({
    queryKey: [...keys.queue(courseId), "full"],
    enabled,
    queryFn: () => unwrap(api.GET("/flash/queue", { params: { query: courseId ? { courseId } : {} } })),
    staleTime: Infinity,
  });
}

export function fetchCustom(body: Schemas["CustomInput"]) {
  return unwrap(api.POST("/flash/custom", { body }));
}

export function useCards(courseId: string, componentId?: string) {
  return useQuery({
    queryKey: [...keys.cards(courseId), componentId ?? "all"],
    queryFn: () =>
      unwrap(
        api.GET("/courses/{id}/flashcards", {
          params: { path: { id: courseId }, query: componentId ? { componentId } : {} },
        }),
      ),
  });
}

/** Everything that counts or lists cards. */
function after(qc: ReturnType<typeof useQueryClient>) {
  void qc.invalidateQueries({ queryKey: ["flash"] });
  void qc.invalidateQueries({ queryKey: ["next"] });
}

export function useCreateCard(courseId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: Schemas["CardInput"]) =>
      unwrap(api.POST("/courses/{id}/flashcards", { params: { path: { id: courseId } }, body })),
    onSuccess: () => after(qc),
  });
}

export function usePatchCard() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ card, body }: { card: Card; body: Schemas["CardPatch"] }) =>
      unwrap(api.PATCH("/flashcards/{id}", { params: { path: { id: card.id }, header: ifMatch(card.version) }, body })),
    onSuccess: () => after(qc),
  });
}

export function useCardFlags() {
  const qc = useQueryClient();
  const suspend = useMutation({
    mutationFn: ({ id, suspended }: { id: string; suspended: boolean }) =>
      unwrap(api.POST("/flashcards/{id}/suspend", { params: { path: { id } }, body: { suspended } })),
    onSuccess: () => after(qc),
  });
  const warmup = useMutation({
    mutationFn: ({ id, flagged }: { id: string; flagged: boolean }) =>
      unwrap(api.POST("/flashcards/{id}/warmup-flag", { params: { path: { id } }, body: { flagged } })),
    onSuccess: () => after(qc),
  });
  return { suspend, warmup };
}

export function findByCode(code: string) {
  return unwrap(api.GET("/flash/cards", { params: { query: { code } } }));
}

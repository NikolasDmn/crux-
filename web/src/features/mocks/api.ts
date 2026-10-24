import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, idempotencyKey, unwrap, type Schemas } from "@/lib/api";
import { clockOffset } from "@/lib/time";

export type Mock = Schemas["MockView"];
export type MockSummary = Schemas["MockSummary"];
export type MockInput = Schemas["MockInput"];
export type Item = Schemas["ItemView"];
export type AnswerInput = Schemas["AnswerInput"];

export const keys = {
  list: (courseId: string) => ["mocks", courseId] as const,
  mock: (id: string) => ["mock", id] as const,
  result: (id: string) => ["mock", id, "result"] as const,
  confidence: (courseId: string) => ["stats", courseId, "confidence"] as const,
};

export function useMocks(courseId: string) {
  return useQuery({
    queryKey: keys.list(courseId),
    queryFn: () => unwrap(api.GET("/courses/{id}/mocks", { params: { path: { id: courseId } } })),
  });
}

export function useConfidence(courseId: string) {
  return useQuery({
    queryKey: keys.confidence(courseId),
    queryFn: () => unwrap(api.GET("/courses/{id}/stats/confidence", { params: { path: { id: courseId } } })),
    staleTime: 60_000,
  });
}

export function useCreateMock(courseId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: MockInput) =>
      unwrap(
        api.POST("/courses/{id}/mocks", {
          params: { path: { id: courseId }, header: { "Idempotency-Key": idempotencyKey() } },
          body,
        }),
      ),
    // Not cached as the mock: the taking screen fetches it to read the server clock (Date header).
    onSuccess: () => void qc.invalidateQueries({ queryKey: keys.list(courseId) }),
  });
}

/** The mock and the server-minus-local clock offset from its `Date` header (timer, spec 5.3). */
export async function fetchMock(id: string): Promise<{ mock: Mock; offset: number }> {
  const r = await api.GET("/mocks/{id}", { params: { path: { id } } });
  const mock = await unwrap(Promise.resolve(r));
  return { mock, offset: clockOffset(r.response.headers.get("Date")) };
}

export function useMock(id: string) {
  return useQuery({ queryKey: keys.mock(id), queryFn: () => fetchMock(id), staleTime: Infinity });
}

export function useResult(id: string) {
  return useQuery({
    queryKey: keys.result(id),
    queryFn: () => unwrap(api.GET("/mocks/{id}/result", { params: { path: { id } } })),
  });
}

export async function startMock(id: string): Promise<{ mock: Mock; offset: number }> {
  const r = await api.POST("/mocks/{id}/start", { params: { path: { id } } });
  const mock = await unwrap(Promise.resolve(r));
  return { mock, offset: clockOffset(r.response.headers.get("Date")) };
}

export function saveAnswer(id: string, order: number, body: AnswerInput) {
  return unwrap(api.PUT("/mocks/{id}/questions/{order}/answer", { params: { path: { id, order } }, body }));
}

export function submitMock(id: string) {
  return unwrap(api.POST("/mocks/{id}/submit", { params: { path: { id } } }));
}

type ReviewAction =
  | { kind: "grade"; order: number; outcome: "Right" | "Partial" | "Wrong" }
  | { kind: "show"; order: number }
  | { kind: "fixed"; order: number };

/** Result-screen actions; each returns the whole mock, which replaces the cached result. */
export function useReview(id: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (a: ReviewAction) => {
      const path = { params: { path: { id, order: a.order } } };
      if (a.kind === "grade")
        return unwrap(api.POST("/mocks/{id}/questions/{order}/grade", { ...path, body: { outcome: a.outcome } }));
      if (a.kind === "show") return unwrap(api.POST("/mocks/{id}/questions/{order}/show-answer", path));
      return unwrap(api.POST("/mocks/{id}/questions/{order}/fixed", path));
    },
    onSuccess: (m) => {
      qc.setQueryData(keys.result(id), m);
      void qc.invalidateQueries({ queryKey: keys.list(m.courseId) });
    },
  });
}

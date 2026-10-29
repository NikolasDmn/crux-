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

// AI grading (frontend-spec 4, mock result): suggestions from the ai service, decided in core.

export type AiGrade = Schemas["GradeView"];

/** Suggestions for this mock. While open answers still wait for one, it polls (the ai service
 *  grades after `mock.submitted`). */
export function useAiGrades(mock: Mock | undefined) {
  const on = mock?.gradingMode === "AI";
  return useQuery({
    queryKey: ["ai", "grades", mock?.id],
    enabled: on,
    queryFn: () => unwrap(api.GET("/ai/grades", { params: { query: { mockId: mock!.id } } })),
    refetchInterval: (q) => {
      const open = (mock?.items ?? []).filter((i) => i.type === "Open" && !i.outcome).map((i) => i.order);
      const have = new Set((q.state.data ?? []).map((g) => g.order));
      return open.some((o) => !have.has(o)) ? 4000 : false;
    },
    retry: false,
  });
}

type Decision =
  | { kind: "accept" | "reject"; order: number; gradeId: string }
  | { kind: "override"; order: number; gradeId: string; outcome: "Right" | "Partial" | "Wrong" }
  | { kind: "acceptAll" };

export function useAiDecision(id: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (d: Decision) => {
      if (d.kind === "acceptAll")
        return unwrap(api.POST("/mocks/{id}/ai-grades/accept-all", { params: { path: { id } } }));
      const path = { params: { path: { id, order: d.order } } };
      if (d.kind !== "override")
        return unwrap(
          api.POST(
            d.kind === "accept"
              ? "/mocks/{id}/questions/{order}/ai-grade/accept"
              : "/mocks/{id}/questions/{order}/ai-grade/reject",
            { ...path, body: { gradeId: d.gradeId } },
          ),
        );
      return unwrap(
        api.POST("/mocks/{id}/questions/{order}/ai-grade/override", {
          ...path,
          body: { gradeId: d.gradeId, outcome: d.outcome },
        }),
      );
    },
    onSuccess: (m) => {
      qc.setQueryData(keys.result(id), m);
      void qc.invalidateQueries({ queryKey: keys.list(m.courseId) });
      // The decision reaches the ai service over the bus.
      setTimeout(() => void qc.invalidateQueries({ queryKey: ["ai", "grades", id] }), 1500);
    },
  });
}

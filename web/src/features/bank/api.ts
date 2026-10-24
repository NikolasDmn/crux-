import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, idempotencyKey, ifMatch, unwrap, type Schemas } from "@/lib/api";
import type { BankFilters } from "@/features/bank/lib/filters";
import { toQuery } from "@/features/bank/lib/filters";

export type Question = Schemas["QuestionView"];
export type QuestionInput = Schemas["QuestionInput"];
export type QuestionPatch = Schemas["QuestionPatch"];
export type VersionInput = Schemas["VersionInput"];
export type Version = Schemas["VersionView"];
export type Paper = Schemas["PaperView"];
export type PaperFields = Schemas["PaperFields"];
export type Concept = Schemas["ConceptView"];

/** A fresh Idempotency-Key, made when a create form opens (spec 2.2). */
export const idempotencyKeyFor = idempotencyKey;

export const keys = {
  bank: (courseId: string) => ["bank", courseId] as const,
  list: (courseId: string, f: BankFilters) => ["bank", courseId, "list", f] as const,
  question: (id: string) => ["question", id] as const,
  versions: (id: string) => ["question", id, "versions"] as const,
  concepts: (courseId: string) => ["bank", courseId, "concepts"] as const,
  papers: (courseId: string) => ["papers", courseId] as const,
};

/** The bank, a page at a time (cursor paging). */
export function useQuestions(courseId: string, f: BankFilters) {
  return useInfiniteQuery({
    queryKey: keys.list(courseId, f),
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }) =>
      unwrap(
        api.GET("/courses/{id}/questions", {
          params: {
            path: { id: courseId },
            query: { ...toQuery(f), limit: 100, ...(pageParam ? { cursor: pageParam } : {}) },
          },
        }),
      ),
    getNextPageParam: (last) => last.nextCursor ?? null,
  });
}

export function useQuestion(id: string | undefined) {
  return useQuery({
    queryKey: keys.question(id ?? ""),
    enabled: !!id,
    queryFn: () => unwrap(api.GET("/questions/{id}", { params: { path: { id: id! } } })),
  });
}

export function useVersions(id: string) {
  return useQuery({
    queryKey: keys.versions(id),
    queryFn: () => unwrap(api.GET("/questions/{id}/versions", { params: { path: { id } } })),
  });
}

function after(qc: ReturnType<typeof useQueryClient>, q: Question) {
  qc.setQueryData(keys.question(q.id), q);
  void qc.invalidateQueries({ queryKey: keys.bank(q.courseId) });
  void qc.invalidateQueries({ queryKey: keys.versions(q.id) });
}

export function useCreateQuestion(courseId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ body, key }: { body: QuestionInput; key: string }) =>
      unwrap(
        api.POST("/courses/{id}/questions", {
          params: { path: { id: courseId }, header: { "Idempotency-Key": key } },
          body,
        }),
      ),
    onSuccess: (q) => after(qc, q),
  });
}

export function usePatchQuestion() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, version, body }: { id: string; version: number; body: QuestionPatch }) =>
      unwrap(api.PATCH("/questions/{id}", { params: { path: { id }, header: ifMatch(version) }, body })),
    onSuccess: (q) => after(qc, q),
  });
}

type Action = "approve" | "archive" | "promote";

/** Approve a draft, archive, or promote a private question into the course bank. */
export function useQuestionAction() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, action }: { id: string; action: Action }) => {
      const path = { params: { path: { id } } };
      if (action === "approve") return unwrap(api.POST("/questions/{id}/approve", path));
      if (action === "archive") return unwrap(api.POST("/questions/{id}/archive", path));
      return unwrap(api.POST("/questions/{id}/promote", path));
    },
    onSuccess: (q) => after(qc, q),
  });
}

export function useConcepts(courseId: string) {
  return useQuery({
    queryKey: keys.concepts(courseId),
    queryFn: () => unwrap(api.GET("/courses/{id}/concepts", { params: { path: { id: courseId } } })),
    staleTime: 5 * 60_000,
  });
}

export function useCreateConcept(courseId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (name: string) =>
      unwrap(api.POST("/courses/{id}/concepts", { params: { path: { id: courseId } }, body: { name } })),
    onSuccess: () => void qc.invalidateQueries({ queryKey: keys.concepts(courseId) }),
  });
}

export function usePapers(courseId: string) {
  return useQuery({
    queryKey: keys.papers(courseId),
    queryFn: () => unwrap(api.GET("/courses/{id}/papers", { params: { path: { id: courseId } } })),
  });
}

export function useSavePaper(courseId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ paper, body }: { paper?: Paper; body: PaperFields }) =>
      paper
        ? unwrap(
            api.PATCH("/papers/{id}", { params: { path: { id: paper.id }, header: ifMatch(paper.version) }, body }),
          )
        : unwrap(
            api.POST("/courses/{id}/papers", {
              params: { path: { id: courseId }, header: { "Idempotency-Key": idempotencyKey() } },
              body,
            }),
          ),
    onSuccess: () => void qc.invalidateQueries({ queryKey: keys.papers(courseId) }),
  });
}

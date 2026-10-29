import { useMutation, useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { api, ApiError, unwrap, type Schemas } from "@/lib/api";
import { readEvents, type SseEvent } from "@/lib/stream";

export type AiSettings = Schemas["AiSettingsView"];
export type AiSettingsPatch = Schemas["AiSettingsPatch"];
export type AiKey = Schemas["AiKeyView"];
export type Tutor = Schemas["TutorView"];
export type TutorInput = Schemas["TutorInput"];
export type MonthUsage = Schemas["MonthUsage"];
export type CourseAi = Schemas["CourseSettingsView"];
export type AiFeature = "MaterialChat" | "Grading" | "CourseImport" | "PastExamImport" | "Generation";
export type AiReason =
  "user_off" | "feature_off" | "course_off" | "material_blocked" | "no_key" | "service_unavailable";

export const keys = {
  settings: ["ai", "settings"] as const,
  keys: ["ai", "keys"] as const,
  tutors: ["ai", "tutors"] as const,
  usage: ["ai", "usage"] as const,
  course: (id: string) => ["ai", "course", id] as const,
  allowed: ["ai-allowed"] as const,
};

/** Anything that can open or close a gate: the gate is asked again everywhere. */
function gateChanged(qc: QueryClient) {
  void qc.invalidateQueries({ queryKey: keys.allowed });
}

/**
 * The AI gate (frontend-spec 2.5): `allowed`, or the `reason` it's closed. An unreachable ai service
 * reads as `service_unavailable`, so AI controls hide and the rest keeps working.
 */
export function useAi(feature: AiFeature, courseId?: string) {
  const q = useQuery({
    queryKey: [...keys.allowed, feature, courseId ?? null],
    queryFn: () =>
      unwrap(api.GET("/me/ai-allowed", { params: { query: { feature, ...(courseId ? { courseId } : {}) } } })),
    staleTime: 60_000,
    retry: false,
  });
  const reason = (q.isError ? "service_unavailable" : (q.data?.reason ?? null)) as AiReason | null;
  return {
    loading: q.isPending,
    allowed: q.data?.allowed === true,
    reason,
    /** Whether the control shows at all: hidden when AI is off for the user or the service is down. */
    visible: q.data?.allowed === true || (reason !== null && reason !== "user_off" && reason !== "service_unavailable"),
  };
}

export function useAiSettings() {
  return useQuery({ queryKey: keys.settings, queryFn: () => unwrap(api.GET("/ai/settings")) });
}

export function usePatchAiSettings() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: AiSettingsPatch) => unwrap(api.PATCH("/ai/settings", { body })),
    onSuccess: (v) => {
      qc.setQueryData(keys.settings, v);
      void qc.invalidateQueries({ queryKey: keys.keys });
      gateChanged(qc);
    },
  });
}

export function useKeys() {
  return useQuery({ queryKey: keys.keys, queryFn: () => unwrap(api.GET("/ai/keys")) });
}

function useKeyMutation<T, R>(fn: (input: T) => Promise<R>) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: fn,
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: keys.keys });
      void qc.invalidateQueries({ queryKey: keys.settings });
      gateChanged(qc);
    },
  });
}

export function useAddKey() {
  return useKeyMutation((body: Schemas["KeyInput"]) => unwrap(api.POST("/ai/keys", { body })));
}

export function useVerifyKey() {
  return useKeyMutation((id: string) =>
    unwrap(api.POST("/ai/keys/{key_id}/verify", { params: { path: { key_id: id } } })),
  );
}

export function useRevokeKey() {
  return useKeyMutation((id: string) => unwrap(api.DELETE("/ai/keys/{key_id}", { params: { path: { key_id: id } } })));
}

export function useTutors(enabled = true) {
  return useQuery({ queryKey: keys.tutors, enabled, queryFn: () => unwrap(api.GET("/ai/tutors")) });
}

export function useSaveTutor() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, body }: { id?: string; body: TutorInput }) =>
      id
        ? unwrap(api.PATCH("/ai/tutors/{tutor_id}", { params: { path: { tutor_id: id } }, body }))
        : unwrap(api.POST("/ai/tutors", { body })),
    onSuccess: () => qc.invalidateQueries({ queryKey: keys.tutors }),
  });
}

export function useUsage(months = 6) {
  return useQuery({
    queryKey: [...keys.usage, months],
    queryFn: () => unwrap(api.GET("/ai/usage", { params: { query: { months } } })),
  });
}

export function useCourseAi(courseId: string, enabled = true) {
  return useQuery({
    queryKey: keys.course(courseId),
    enabled,
    queryFn: () => unwrap(api.GET("/ai/courses/{course_id}/settings", { params: { path: { course_id: courseId } } })),
  });
}

export function usePutCourseAi(courseId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: Schemas["CourseSettingsInput"]) =>
      unwrap(api.PUT("/ai/courses/{course_id}/settings", { params: { path: { course_id: courseId } }, body })),
    onSuccess: (v) => {
      qc.setQueryData(keys.course(courseId), v);
      gateChanged(qc);
    },
  });
}

// Ask AI (frontend-spec 5.5).

export type Conversation = Schemas["ConversationView"];
export type Message = Schemas["MessageView"];

export function useConversations(courseId: string, scope: string, materialId?: string, enabled = true) {
  return useQuery({
    queryKey: ["ai", "conversations", courseId, scope, materialId ?? null],
    enabled,
    queryFn: () =>
      unwrap(
        api.GET("/ai/conversations", {
          params: { query: { courseId, scope, ...(materialId ? { materialId } : {}) } },
        }),
      ),
  });
}

export function useConversation(id: string | null) {
  return useQuery({
    queryKey: ["ai", "conversation", id],
    enabled: !!id,
    queryFn: () =>
      unwrap(api.GET("/ai/conversations/{conversation_id}", { params: { path: { conversation_id: id! } } })),
  });
}

export function createConversation(body: Schemas["ConversationInput"]) {
  return unwrap(api.POST("/ai/conversations", { body }));
}

/**
 * Sends a message and yields the reply's events as they arrive (2.3: the body of the POST, read
 * with a stream reader). Gate, budget and rate-limit errors throw before the first event.
 */
export async function* sendMessage(
  conversationId: string,
  body: Schemas["SendInput"],
  signal?: AbortSignal,
): AsyncGenerator<SseEvent> {
  const { response, error } = await api.POST("/ai/conversations/{conversation_id}/messages", {
    params: { path: { conversation_id: conversationId } },
    body,
    parseAs: "stream",
    ...(signal ? { signal } : {}),
  });
  if (!response.ok) throw ApiError.from(response.status, error);
  if (!response.body) return;
  yield* readEvents(response.body);
}

export function useFeedback(conversationId: string | null) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ messageId, feedback }: { messageId: number; feedback: "Up" | "Down" | null }) =>
      unwrap(
        api.POST("/ai/messages/{message_id}/feedback", {
          params: { path: { message_id: messageId } },
          body: { feedback },
        }),
      ),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["ai", "conversation", conversationId] }),
  });
}

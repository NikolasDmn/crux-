import { useMutation, useQueries, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, idempotencyKey, ifMatch, unwrap, type Schemas } from "@/lib/api";
import { useSettings } from "@/features/plan/api";
import { readChosen, shownScales } from "./lib/scales";

export type Course = Schemas["CourseView"];
export type CourseFields = Schemas["CourseFields"];
export type Invitation = Schemas["InvitationView"];
export type Deadline = Schemas["DeadlineView"];
export type Members = Schemas["MembersView"];
export type GradeScale = Schemas["GradeScaleView"];

export const keys = {
  courses: ["courses"] as const,
  trash: ["courses", "trash"] as const,
  course: (id: string) => ["course", id] as const,
  members: (id: string) => ["course", id, "members"] as const,
  invitations: ["course-invitations"] as const,
  deadlines: (courseId: string) => ["course", courseId, "deadlines"] as const,
  scales: ["grade-scales"] as const,
};

export function useCourses() {
  return useQuery({ queryKey: keys.courses, queryFn: () => unwrap(api.GET("/courses")) });
}

export function useTrash() {
  return useQuery({
    queryKey: keys.trash,
    queryFn: () => unwrap(api.GET("/courses", { params: { query: { trashed: true } } })),
  });
}

export function useCourse(id: string) {
  return useQuery({
    queryKey: keys.course(id),
    queryFn: () => unwrap(api.GET("/courses/{id}", { params: { path: { id } } })),
  });
}

/** Everything that lists courses or shows one. */
function invalidateCourses(qc: ReturnType<typeof useQueryClient>, id?: string) {
  void qc.invalidateQueries({ queryKey: keys.courses });
  if (id) void qc.invalidateQueries({ queryKey: keys.course(id) });
}

export function useCreateCourse() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ fields, key }: { fields: CourseFields; key: string }) =>
      unwrap(api.POST("/courses", { body: fields, params: { header: { "Idempotency-Key": key } } })),
    onSuccess: (c) => {
      qc.setQueryData(keys.course(c.id), c);
      invalidateCourses(qc);
    },
  });
}

export function usePatchCourse(id: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ fields, version }: { fields: CourseFields; version: number }) =>
      unwrap(api.PATCH("/courses/{id}", { params: { path: { id }, header: ifMatch(version) }, body: fields })),
    onSuccess: (c) => {
      qc.setQueryData(keys.course(id), c);
      invalidateCourses(qc);
    },
  });
}

/** Moves the course to the trash (D2-5): 30 days, Owner can restore. */
export function useTrashCourse(id: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => unwrap(api.DELETE("/courses/{id}", { params: { path: { id } } })),
    onSuccess: () => {
      invalidateCourses(qc, id);
      void qc.invalidateQueries({ queryKey: keys.trash });
    },
  });
}

export function useRestoreCourse() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => unwrap(api.POST("/courses/{id}/restore", { params: { path: { id } } })),
    onSuccess: (c) => {
      invalidateCourses(qc, c.id);
      void qc.invalidateQueries({ queryKey: keys.trash });
    },
  });
}

/** Subscribe (Active), archive (Archived), or unsubscribe (null). */
export function useSubscription(id: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (status: "Active" | "Archived" | null) => {
      if (status === null) await unwrap(api.DELETE("/courses/{id}/subscription", { params: { path: { id } } }));
      else await unwrap(api.POST("/courses/{id}/subscription", { params: { path: { id } }, body: { status } }));
    },
    onSuccess: () => invalidateCourses(qc, id),
  });
}

export function useMembers(id: string, enabled = true) {
  return useQuery({
    queryKey: keys.members(id),
    queryFn: () => unwrap(api.GET("/courses/{id}/members", { params: { path: { id } } })),
    enabled,
  });
}

export function useShare(id: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: Schemas["ShareInput"]) =>
      unwrap(api.POST("/courses/{id}/members", { params: { path: { id } }, body })),
    onSuccess: (m) => qc.setQueryData(keys.members(id), m),
  });
}

/** Removes a member, withdraws an email share, or (your own user id) leaves the course. */
export function useRemoveMember(id: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (memberId: string) =>
      unwrap(api.DELETE("/courses/{id}/members/{memberId}", { params: { path: { id, memberId } } })),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: keys.members(id) });
      invalidateCourses(qc, id);
    },
  });
}

export function useGradeScales() {
  return useQuery({ queryKey: keys.scales, queryFn: () => unwrap(api.GET("/grade-scales")), staleTime: 5 * 60_000 });
}

/** The scales a picker offers: the ones chosen in Settings > Study (all until chosen), your own,
 *  and `keep` (a course's current scale). */
export function useShownGradeScales(keep?: string | null) {
  const scales = useGradeScales();
  const settings = useSettings();
  return { ...scales, data: scales.data && shownScales(scales.data, readChosen(settings.data?.extra), keep) };
}

export function useInvitations() {
  return useQuery({ queryKey: keys.invitations, queryFn: () => unwrap(api.GET("/course-invitations")) });
}

export function useAnswerInvitation() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ courseId, accept }: { courseId: string; accept: boolean }) =>
      accept
        ? unwrap(api.POST("/course-invitations/{courseId}/accept", { params: { path: { courseId } } }))
        : unwrap(api.POST("/course-invitations/{courseId}/decline", { params: { path: { courseId } } })),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: keys.invitations });
      invalidateCourses(qc);
    },
  });
}

export function useDeadlines(courseId: string, enabled = true) {
  return useQuery({
    queryKey: keys.deadlines(courseId),
    queryFn: () => unwrap(api.GET("/courses/{id}/deadlines", { params: { path: { id: courseId } } })),
    enabled,
    staleTime: 60_000,
  });
}

/** The next upcoming exam or resit per course (sidebar countdown, exams on Home). */
export function useNextExams(courses: Course[] | undefined) {
  const subscribed = (courses ?? []).filter((c) => c.mySubscription === "Active");
  const results = useQueries({
    queries: subscribed.map((c) => ({
      queryKey: keys.deadlines(c.id),
      queryFn: () => unwrap(api.GET("/courses/{id}/deadlines", { params: { path: { id: c.id } } })),
      staleTime: 60_000,
    })),
  });
  const next: Record<string, Deadline | undefined> = {};
  subscribed.forEach((c, i) => {
    next[c.id] = nextExam(results[i]?.data ?? []);
  });
  return next;
}

export function nextExam(deadlines: Deadline[], now: Date = new Date()): Deadline | undefined {
  return deadlines
    .filter((d) => (d.type === "Exam" || d.type === "Resit") && d.status === "Upcoming" && new Date(d.startsAt) > now)
    .sort((a, b) => a.startsAt.localeCompare(b.startsAt))[0];
}

export { idempotencyKey };

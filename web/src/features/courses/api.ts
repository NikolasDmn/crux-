import { useMutation, useQueries, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, unwrap, type Schemas } from "@/lib/api";

export type Course = Schemas["CourseView"];
export type Invitation = Schemas["InvitationView"];
export type Deadline = Schemas["DeadlineView"];

export const keys = {
  courses: ["courses"] as const,
  trash: ["courses", "trash"] as const,
  invitations: ["course-invitations"] as const,
  deadlines: (courseId: string) => ["course", courseId, "deadlines"] as const,
};

export function useCourses() {
  return useQuery({ queryKey: keys.courses, queryFn: () => unwrap(api.GET("/courses")) });
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
      void qc.invalidateQueries({ queryKey: keys.courses });
    },
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

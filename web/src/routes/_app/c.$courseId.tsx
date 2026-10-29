import { createFileRoute, Link, Outlet } from "@tanstack/react-router";
import type { CSSProperties } from "react";
import { EmptyList } from "@/components/ui/empty";
import { Dot, Skeleton } from "@/components/ui/misc";
import { useCourse, useDeadlines, nextExam } from "@/features/courses/api";
import { ApiError } from "@/lib/api";
import { relativeDays } from "@/lib/time";
import { useUser } from "@/app/auth";
import { CourseAskButton } from "@/features/ai/course-ask";

// Every course page (frontend-spec 4, 5.2): --course set on the layout root, a header (with Ask AI)
// and the tabs.
export const Route = createFileRoute("/_app/c/$courseId")({ component: CourseLayout });

const TABS: { to: string; label: string; exact?: boolean }[] = [
  { to: "/c/$courseId", label: "Overview", exact: true },
  { to: "/c/$courseId/lectures", label: "Lectures" },
  { to: "/c/$courseId/bank", label: "Questions" },
  { to: "/c/$courseId/papers", label: "Past papers" },
  { to: "/c/$courseId/mocks", label: "Mocks" },
  { to: "/c/$courseId/cards", label: "Cards" },
  { to: "/c/$courseId/plan", label: "Plan" },
  { to: "/c/$courseId/exams", label: "Exams" },
  { to: "/c/$courseId/settings", label: "Settings" },
];

function CourseLayout() {
  const { courseId } = Route.useParams();
  const user = useUser();
  const course = useCourse(courseId);
  const deadlines = useDeadlines(courseId, course.data?.mySubscription === "Active");
  const exam = nextExam(deadlines.data ?? []);

  if (course.isError) {
    const gone = course.error instanceof ApiError && course.error.status === 404;
    return <EmptyList title={gone ? "Not found, or you don't have access." : "This course couldn't load."} />;
  }
  const style = {
    "--course": course.data?.color ?? "var(--text-muted)",
    "--course-soft": "color-mix(in srgb, var(--course) 16%, var(--surface))",
    "--tab-accent": "var(--course)",
  } as CSSProperties;

  return (
    <div className="flex flex-col gap-5" style={style}>
      <header className="flex flex-col gap-3">
        {course.data ? (
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
            <Dot className="size-2.5" />
            <h1 className="m-0 text-xl font-semibold tracking-[-0.01em]">{course.data.name}</h1>
            <span className="font-mono text-sm text-muted">{course.data.code}</span>
            <span className="ml-auto flex items-center gap-3">
              {exam ? (
                <span className="text-sm text-muted">
                  {exam.type === "Resit" ? "Resit" : "Exam"} {relativeDays(exam.startsAt, user.timeZone)}
                </span>
              ) : null}
              <CourseAskButton courseId={courseId} />
            </span>
          </div>
        ) : (
          <Skeleton className="h-7 w-72" />
        )}
        <nav
          aria-label="Course"
          className="-mx-1 flex gap-5 overflow-x-auto overflow-y-hidden border-b border-border px-1"
        >
          {TABS.map((t) => (
            <Link
              key={t.to}
              to={t.to}
              params={{ courseId }}
              activeOptions={{ exact: t.exact ?? false }}
              className="-mb-px whitespace-nowrap border-b-2 border-transparent py-2 text-muted hover:text-text data-[status=active]:border-course data-[status=active]:font-medium data-[status=active]:text-text"
            >
              {t.label}
            </Link>
          ))}
        </nav>
      </header>
      <Outlet />
    </div>
  );
}

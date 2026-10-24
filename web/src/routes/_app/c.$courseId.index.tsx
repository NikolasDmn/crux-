import { createFileRoute, Link } from "@tanstack/react-router";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Card, SectionLabel } from "@/components/ui/card";
import { EmptyList } from "@/components/ui/empty";
import { Skeleton } from "@/components/ui/misc";
import { ProgressBar } from "@/components/ui/progress";
import { useCourse, useSubscription } from "@/features/courses/api";
import { canEdit } from "@/features/courses/lib/roles";
import { useLectures, useProgress } from "@/features/lectures/api";
import { messageFor } from "@/lib/errors";

// Course overview. Step 2: subscription, lecture progress and hours. The stats, weak spots, traps and
// exam format arrive with steps 3–4.
export const Route = createFileRoute("/_app/c/$courseId/")({ component: Overview });

function Overview() {
  const { courseId } = Route.useParams();
  const course = useCourse(courseId);
  const lectures = useLectures(courseId);
  const subscribed = course.data?.mySubscription === "Active";
  const progress = useProgress(courseId);
  const subscribe = useSubscription(courseId);

  if (course.isPending || lectures.isPending) return <Skeleton className="h-48" />;
  const done = (progress.data ?? []).filter((p) => p.status === "Done").length;
  const actual = (progress.data ?? []).reduce((h, p) => h + p.actualHours, 0);
  const estimate = (lectures.data ?? []).reduce((h, l) => h + (l.estHours ?? 1.5), 0);

  return (
    <div className="flex flex-col gap-4">
      {!subscribed ? (
        <Banner
          tone="notice"
          action={
            <Button
              size="sm"
              variant="primary"
              disabled={subscribe.isPending}
              onClick={() => subscribe.mutate("Active")}
            >
              Subscribe
            </Button>
          }
        >
          {course.data?.mySubscription === "Archived"
            ? "This course is archived for you. Subscribe again to plan and track it."
            : "Subscribe to track your progress, plan your study and get reminders."}
          {subscribe.error ? <span className="block text-danger-text">{messageFor(subscribe.error)}</span> : null}
        </Banner>
      ) : null}
      <div className="grid gap-4 md:grid-cols-2">
        <Card className="flex flex-col gap-4">
          <SectionLabel>Progress</SectionLabel>
          {lectures.data?.length ? (
            <ProgressBar
              value={done}
              max={lectures.data.length}
              label={`Lectures ${done} / ${lectures.data.length}`}
              detail={`${actual.toFixed(1)} of ${estimate.toFixed(0)} h`}
            />
          ) : (
            <EmptyList
              title="No lectures yet"
              body="Add the course's lectures or weeks; materials and questions hang off them."
              action={
                canEdit(course.data?.myRole) ? (
                  <Button asChild size="sm" variant="primary">
                    <Link to="/c/$courseId/lectures" params={{ courseId }}>
                      Add lectures
                    </Link>
                  </Button>
                ) : undefined
              }
            />
          )}
        </Card>
        <Card className="flex flex-col gap-3">
          <SectionLabel>About</SectionLabel>
          <dl className="m-0 grid grid-cols-[auto_1fr] gap-x-6 gap-y-1.5 text-sm">
            <dt className="text-muted">Owner</dt>
            <dd className="m-0">{course.data?.owner.name}</dd>
            <dt className="text-muted">Your role</dt>
            <dd className="m-0">{course.data?.myRole}</dd>
            <dt className="text-muted">Grade scale</dt>
            <dd className="m-0">{course.data?.gradeScale.name}</dd>
            {course.data?.institution ? (
              <>
                <dt className="text-muted">Institution</dt>
                <dd className="m-0">{course.data.institution}</dd>
              </>
            ) : null}
            {course.data?.term ? (
              <>
                <dt className="text-muted">Term</dt>
                <dd className="m-0">{course.data.term}</dd>
              </>
            ) : null}
          </dl>
        </Card>
      </div>
    </div>
  );
}

import { createFileRoute, Link } from "@tanstack/react-router";
import { useUser } from "@/app/auth";
import { Button } from "@/components/ui/button";
import { Card, SectionLabel } from "@/components/ui/card";
import { Checklist } from "@/components/ui/empty";
import { Skeleton } from "@/components/ui/misc";
import { useCourses, useNextExams } from "@/features/courses/api";
import { formatInZone } from "@/lib/time";
import { t } from "@/lib/t";

// Home. Build step 1 ships the greeting and the first-run checklist; the full Home (plan, Take me
// there card, check-ins, course cards) comes with step 3.
export const Route = createFileRoute("/_app/")({ component: Home });

function Home() {
  const user = useUser();
  const courses = useCourses();
  const live = (courses.data ?? []).filter((c) => !c.trashedAt);
  const next = useNextExams(live);
  const hasExam = Object.values(next).some(Boolean);

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-wrap items-baseline gap-3">
        <h1 className="m-0 text-xl font-semibold tracking-[-0.01em]">
          {t("home.greeting", { name: user.firstName || user.username })}
        </h1>
        <span className="text-sm text-muted">{formatInZone(new Date(), user.timeZone, "EEE d MMM")}</span>
      </div>
      {courses.isPending ? (
        <Skeleton className="h-40" />
      ) : !live.length || !hasExam ? (
        <Card className="flex max-w-[560px] flex-col gap-4">
          <SectionLabel>Get started</SectionLabel>
          <Checklist
            steps={[
              {
                label: "Create your first course",
                done: live.length > 0,
                action: (
                  <Button asChild size="sm" variant="primary">
                    <Link to="/courses/new">{t("nav.newCourse")}</Link>
                  </Button>
                ),
              },
              {
                label: "Add the exam date",
                done: hasExam,
                action: live[0] ? (
                  <Button asChild size="sm">
                    <Link to="/c/$courseId" params={{ courseId: live[0].id }}>
                      Open {live[0].code}
                    </Link>
                  </Button>
                ) : undefined,
              },
              { label: "Upload the slides", done: false },
            ]}
          />
        </Card>
      ) : (
        <Card>
          <SectionLabel>Your courses</SectionLabel>
          <ul className="m-0 mt-3 flex list-none flex-col gap-2 p-0">
            {live.map((c) => (
              <li key={c.id}>
                <Link
                  to="/c/$courseId"
                  params={{ courseId: c.id }}
                  className="font-medium underline-offset-4 hover:underline"
                >
                  {c.code} · {c.name}
                </Link>
              </li>
            ))}
          </ul>
        </Card>
      )}
    </div>
  );
}

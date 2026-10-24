import { createFileRoute, Link } from "@tanstack/react-router";
import { ChevronLeft } from "lucide-react";
import { EmptyList } from "@/components/ui/empty";
import { Skeleton } from "@/components/ui/misc";
import { Tab, TabList, TabPanel, Tabs } from "@/components/ui/tabs";
import { useCourse } from "@/features/courses/api";
import { canEdit } from "@/features/courses/lib/roles";
import { useLectures, useProgress } from "@/features/lectures/api";
import { MaterialList } from "@/features/materials/material-list";

// One lecture (frontend-spec 4). Step 2 builds Materials; the other tabs fill in with steps 3–5.
export const Route = createFileRoute("/_app/c/$courseId/l/$componentId")({ component: LecturePage });

function Later({ what, step }: { what: string; step: number }) {
  return <EmptyList title={`${what} arrive with build step ${step}`} />;
}

function LecturePage() {
  const { courseId, componentId } = Route.useParams();
  const course = useCourse(courseId);
  const lectures = useLectures(courseId);
  const progress = useProgress(courseId);
  const lecture = lectures.data?.find((l) => l.id === componentId);
  const p = progress.data?.find((x) => x.componentId === componentId);

  if (lectures.isPending) return <Skeleton className="h-64" />;
  if (!lecture) return <EmptyList title="Not found, or you don't have access." />;

  return (
    <div className="flex flex-col gap-4">
      <Link
        to="/c/$courseId/lectures"
        params={{ courseId }}
        className="inline-flex items-center gap-1 self-start text-sm text-muted hover:text-text"
      >
        <ChevronLeft className="size-4" aria-hidden="true" />
        Lectures
      </Link>
      <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
        <span className="font-mono text-muted">{lecture.code}</span>
        <h2 className="m-0 text-lg font-semibold">{lecture.name}</h2>
        <span className="ml-auto font-mono text-sm text-muted tnum">
          {p ? `${p.actualHours.toFixed(1)} of ` : ""}
          {lecture.estHours ?? 1.5} h
          {p ? ` · ${p.status === "InProgress" ? "in progress" : p.status === "Done" ? "done" : "to do"}` : ""}
        </span>
      </div>
      {lecture.description ? <p className="m-0 max-w-[72ch] text-muted">{lecture.description}</p> : null}
      <Tabs defaultValue="materials">
        <TabList>
          <Tab value="materials">Materials</Tab>
          <Tab value="questions">Questions</Tab>
          <Tab value="cards">Flashcards</Tab>
          <Tab value="slips">Slips</Tab>
          <Tab value="sessions">Sessions</Tab>
        </TabList>
        <TabPanel value="materials" className="pt-4">
          <MaterialList courseId={courseId} componentId={componentId} editor={canEdit(course.data?.myRole)} />
        </TabPanel>
        <TabPanel value="questions" className="pt-4">
          <Later what="Questions" step={4} />
        </TabPanel>
        <TabPanel value="cards" className="pt-4">
          <Later what="Flashcards" step={5} />
        </TabPanel>
        <TabPanel value="slips" className="pt-4">
          <Later what="Slips" step={4} />
        </TabPanel>
        <TabPanel value="sessions" className="pt-4">
          <Later what="Study sessions" step={3} />
        </TabPanel>
      </Tabs>
    </div>
  );
}

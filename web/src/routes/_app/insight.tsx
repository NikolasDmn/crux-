import { createFileRoute, Link, useNavigate } from "@tanstack/react-router";
import { Badge } from "@/components/ui/badge";
import { Card, SectionLabel } from "@/components/ui/card";
import { Select } from "@/components/ui/field";
import { Dot } from "@/components/ui/misc";
import { SeverityBar, type Severity } from "@/components/ui/severity";
import { useCourses } from "@/features/courses/api";
import { SLIP_TYPES, useSlips, useWeakSpots } from "@/features/mistakes/api";
import { SlipList, slipLabel } from "@/features/mistakes/components/slip-list";
import { usePatterns } from "@/features/next/api";

// Mistakes (frontend-spec 4, /insight): slips filtered by course, type and open/resolved; open weak
// spots across courses; the mistakes that recur across courses.
type Search = { courseId?: string; type?: string; status?: "open" | "resolved" };

export const Route = createFileRoute("/_app/insight")({
  validateSearch: (s: Record<string, unknown>): Search => ({
    ...(typeof s.courseId === "string" && s.courseId ? { courseId: s.courseId } : {}),
    ...(typeof s.type === "string" && s.type ? { type: s.type } : {}),
    ...(s.status === "open" || s.status === "resolved" ? { status: s.status } : {}),
  }),
  component: Mistakes,
});

function Mistakes() {
  const search = Route.useSearch();
  const navigate = useNavigate({ from: Route.fullPath });
  const courses = useCourses();
  const live = (courses.data ?? []).filter((c) => !c.trashedAt && c.mySubscription === "Active");
  const codes = Object.fromEntries((courses.data ?? []).map((c) => [c.id, c.code]));
  const slips = useSlips({ courseId: search.courseId, status: search.status ?? "open", type: search.type });
  const patterns = usePatterns();
  const set = (p: Partial<Search>) => void navigate({ search: (old: Search) => ({ ...old, ...p }), replace: true });

  return (
    <div className="flex max-w-[960px] flex-col gap-5">
      <h1 className="m-0 text-xl font-semibold tracking-[-0.01em]">Mistakes</h1>
      {patterns.data?.filter((p) => p.courses.length > 1).length ? (
        <Card className="flex flex-col gap-2">
          <SectionLabel>Across courses</SectionLabel>
          {patterns.data
            .filter((p) => p.courses.length > 1)
            .map((p) => (
              <div key={p.type} className="flex flex-wrap items-center gap-2">
                <span className="font-medium">{slipLabel(p.type)}</span>
                <span className="text-sm text-muted">{p.openSlips} open</span>
                <span className="ml-auto flex gap-1">
                  {p.courses.map((c) => (
                    <Badge key={c.courseId} kind="code">
                      {c.code} {c.count}
                    </Badge>
                  ))}
                </span>
              </div>
            ))}
        </Card>
      ) : null}
      <section className="flex flex-col gap-3">
        <div className="flex flex-wrap items-center gap-2">
          <SectionLabel className="mr-auto">Slips</SectionLabel>
          <Select
            aria-label="Course"
            className="h-8 w-auto text-sm"
            value={search.courseId ?? ""}
            onChange={(e) => set({ courseId: e.currentTarget.value || undefined })}
          >
            <option value="">All courses</option>
            {live.map((c) => (
              <option key={c.id} value={c.id}>
                {c.code}
              </option>
            ))}
          </Select>
          <Select
            aria-label="Kind"
            className="h-8 w-auto text-sm"
            value={search.type ?? ""}
            onChange={(e) => set({ type: e.currentTarget.value || undefined })}
          >
            <option value="">Every kind</option>
            {SLIP_TYPES.map((t) => (
              <option key={t} value={t}>
                {slipLabel(t)}
              </option>
            ))}
          </Select>
          <Select
            aria-label="Status"
            className="h-8 w-auto text-sm"
            value={search.status ?? "open"}
            onChange={(e) => set({ status: e.currentTarget.value as "open" | "resolved" })}
          >
            <option value="open">Open</option>
            <option value="resolved">Resolved</option>
          </Select>
        </div>
        <SlipList slips={slips.data} loading={slips.isPending} codes={search.courseId ? undefined : codes} />
      </section>
      <section className="flex flex-col gap-3">
        <SectionLabel>Open weak spots</SectionLabel>
        <div className="grid gap-4 [grid-template-columns:repeat(auto-fit,minmax(280px,1fr))]">
          {live.map((c) => (
            <CourseSpots key={c.id} courseId={c.id} code={c.code} color={c.color} />
          ))}
        </div>
      </section>
    </div>
  );
}

function CourseSpots({ courseId, code, color }: { courseId: string; code: string; color: string }) {
  const spots = useWeakSpots(courseId);
  const open = spots.data?.open ?? [];
  return (
    <Card className="flex flex-col gap-3">
      <Link to="/c/$courseId" params={{ courseId }} className="flex items-center gap-2 font-medium hover:underline">
        <Dot color={color} />
        <span className="font-mono text-sm">{code}</span>
      </Link>
      {open.length ? (
        open.slice(0, 5).map((w) => <SeverityBar key={w.id} topic={w.topic} severity={w.severity as Severity} />)
      ) : (
        <p className="m-0 text-sm text-muted">None open.</p>
      )}
      {open.length > 5 ? <p className="m-0 text-sm text-muted">and {open.length - 5} more</p> : null}
    </Card>
  );
}

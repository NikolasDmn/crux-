import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { PlanView } from "@/features/plan/components/plan-view";

// This course's plan; `?w=2026-W43` picks the week.
export const Route = createFileRoute("/_app/c/$courseId/plan")({
  validateSearch: (s: Record<string, unknown>): { w?: string } => (typeof s.w === "string" ? { w: s.w } : {}),
  component: CoursePlan,
});

function CoursePlan() {
  const { courseId } = Route.useParams();
  const { w } = Route.useSearch();
  const navigate = useNavigate({ from: Route.fullPath });
  return <PlanView courseId={courseId} isoWeek={w} onWeek={(week) => void navigate({ search: { w: week } })} />;
}

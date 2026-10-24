import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { PlanView } from "@/features/plan/components/plan-view";

// Every course's plan in one week; `?w=2026-W43` picks the week.
export const Route = createFileRoute("/_app/plan")({
  validateSearch: (s: Record<string, unknown>): { w?: string } => (typeof s.w === "string" ? { w: s.w } : {}),
  component: Plan,
});

function Plan() {
  const { w } = Route.useSearch();
  const navigate = useNavigate({ from: Route.fullPath });
  return (
    <div className="flex flex-col gap-4">
      <h1 className="m-0 text-xl font-semibold tracking-[-0.01em]">Plan</h1>
      <PlanView isoWeek={w} onWeek={(week) => void navigate({ search: { w: week } })} />
    </div>
  );
}

import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { WeekView } from "@/features/week/week-view";

// `/week/2026-W43`: an earlier week.
export const Route = createFileRoute("/_app/week/$isoWeek")({ component: AWeek });

function AWeek() {
  const { isoWeek } = Route.useParams();
  const navigate = useNavigate();
  return <WeekView isoWeek={isoWeek} onWeek={(w) => void navigate({ to: "/week/$isoWeek", params: { isoWeek: w } })} />;
}

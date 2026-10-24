import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { WeekView } from "@/features/week/week-view";

export const Route = createFileRoute("/_app/week/")({ component: ThisWeek });

function ThisWeek() {
  const navigate = useNavigate();
  return <WeekView onWeek={(isoWeek) => void navigate({ to: "/week/$isoWeek", params: { isoWeek } })} />;
}

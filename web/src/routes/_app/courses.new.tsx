import { createFileRoute } from "@tanstack/react-router";
import { NotBuilt } from "@/components/shell/not-built";

export const Route = createFileRoute("/_app/courses/new")({
  component: () => <NotBuilt title="New course" step={2} />,
});

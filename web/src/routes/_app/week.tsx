import { createFileRoute } from "@tanstack/react-router";
import { NotBuilt } from "@/components/shell/not-built";

export const Route = createFileRoute("/_app/week")({
  component: () => <NotBuilt title="This week" step={3} />,
});

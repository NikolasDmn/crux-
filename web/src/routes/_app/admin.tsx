import { createFileRoute } from "@tanstack/react-router";
import { NotBuilt } from "@/components/shell/not-built";

export const Route = createFileRoute("/_app/admin")({
  component: () => <NotBuilt title="Admin" step={9} />,
});

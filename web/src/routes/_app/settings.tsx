import { createFileRoute } from "@tanstack/react-router";
import { NotBuilt } from "@/components/shell/not-built";

export const Route = createFileRoute("/_app/settings")({
  component: () => <NotBuilt title="Settings" step={6} />,
});

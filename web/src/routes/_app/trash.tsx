import { createFileRoute } from "@tanstack/react-router";
import { NotBuilt } from "@/components/shell/not-built";

export const Route = createFileRoute("/_app/trash")({
  component: () => <NotBuilt title="Trash" step={2} />,
});

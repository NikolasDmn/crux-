import { createFileRoute } from "@tanstack/react-router";
import { NotBuilt } from "@/components/shell/not-built";

export const Route = createFileRoute("/_app/c/$courseId/cards")({
  component: () => <NotBuilt title="Cards" step={5} />,
});

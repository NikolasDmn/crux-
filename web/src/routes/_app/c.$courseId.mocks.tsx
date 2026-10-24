import { createFileRoute } from "@tanstack/react-router";
import { NotBuilt } from "@/components/shell/not-built";

export const Route = createFileRoute("/_app/c/$courseId/mocks")({
  component: () => <NotBuilt title="Mocks" step={4} />,
});

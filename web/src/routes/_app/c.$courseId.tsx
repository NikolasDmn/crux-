import { createFileRoute } from "@tanstack/react-router";
import { NotBuilt } from "@/components/shell/not-built";

export const Route = createFileRoute("/_app/c/$courseId")({
  component: () => <NotBuilt title="Course" step={2} />,
});

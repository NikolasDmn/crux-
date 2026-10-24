import { createFileRoute } from "@tanstack/react-router";
import { NotBuilt } from "@/components/shell/not-built";

export const Route = createFileRoute("/_app/c/$courseId/exams")({
  component: () => <NotBuilt title="Exams" step={3} />,
});

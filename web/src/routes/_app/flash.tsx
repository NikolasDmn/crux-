import { createFileRoute } from "@tanstack/react-router";
import { NotBuilt } from "@/components/shell/not-built";

export const Route = createFileRoute("/_app/flash")({
  component: () => <NotBuilt title="Flashcards" step={5} />,
});

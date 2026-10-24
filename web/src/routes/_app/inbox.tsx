import { createFileRoute } from "@tanstack/react-router";
import { NotBuilt } from "@/components/shell/not-built";

export const Route = createFileRoute("/_app/inbox")({
  component: () => <NotBuilt title="Inbox" step={6} />,
});

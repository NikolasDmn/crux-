import { createFileRoute } from "@tanstack/react-router";
import { EmptyList } from "@/components/ui/empty";

export const Route = createFileRoute("/_app/settings/ai")({
  component: () => <EmptyList title="Not built yet" body="AI settings arrive with build step 8." />,
});

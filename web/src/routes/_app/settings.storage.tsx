import { createFileRoute } from "@tanstack/react-router";
import { EmptyList } from "@/components/ui/empty";

export const Route = createFileRoute("/_app/settings/storage")({
  component: () => <EmptyList title="Not built yet" body="Storage settings arrive with build step 7." />,
});

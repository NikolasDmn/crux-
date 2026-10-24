import { createFileRoute, Link } from "@tanstack/react-router";
import { Button } from "@/components/ui/button";
import { CardTable } from "@/features/flash/components/card-table";

// Card list (frontend-spec 4): the course's cards and your personal ones; create, edit, suspend.
export const Route = createFileRoute("/_app/c/$courseId/cards")({ component: Cards });

function Cards() {
  const { courseId } = Route.useParams();
  return (
    <div className="flex max-w-[960px] flex-col gap-4">
      <div className="flex justify-end">
        <Button asChild size="sm">
          <Link to="/flash/review" search={{ courseId }}>
            Review this course
          </Link>
        </Button>
      </div>
      <CardTable courseId={courseId} />
    </div>
  );
}

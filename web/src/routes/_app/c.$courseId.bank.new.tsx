import { createFileRoute, Link } from "@tanstack/react-router";
import { ChevronLeft } from "lucide-react";
import { QuestionEditor } from "@/features/bank/components/question-editor";

// New question; `?lecture=` preselects the lecture (from a lecture's Questions tab).
export const Route = createFileRoute("/_app/c/$courseId/bank/new")({
  validateSearch: (s: Record<string, unknown>): { lecture?: string } =>
    typeof s.lecture === "string" ? { lecture: s.lecture } : {},
  component: NewQuestion,
});

function NewQuestion() {
  const { courseId } = Route.useParams();
  const { lecture } = Route.useSearch();
  return (
    <div className="flex max-w-[960px] flex-col gap-4">
      <Link
        to="/c/$courseId/bank"
        params={{ courseId }}
        className="inline-flex items-center gap-1 self-start text-sm text-muted hover:text-text"
      >
        <ChevronLeft className="size-4" aria-hidden="true" />
        Questions
      </Link>
      <h2 className="m-0 text-lg font-semibold">New question</h2>
      <QuestionEditor courseId={courseId} initialLecture={lecture} />
    </div>
  );
}

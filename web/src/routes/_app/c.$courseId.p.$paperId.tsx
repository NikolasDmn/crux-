import { useQuery } from "@tanstack/react-query";
import { createFileRoute, Link } from "@tanstack/react-router";
import { ChevronLeft } from "lucide-react";
import { lazy, Suspense } from "react";
import { Badge } from "@/components/ui/badge";
import { Card } from "@/components/ui/card";
import { EmptyList } from "@/components/ui/empty";
import { Skeleton } from "@/components/ui/misc";
import { usePapers, useQuestions } from "@/features/bank/api";
import { fileUrl } from "@/features/files/api";
import { sameOrigin } from "@/features/files/lib/upload";

// A past paper (frontend-spec 4): what's known about it, and its PDF, like a material's page.
export const Route = createFileRoute("/_app/c/$courseId/p/$paperId")({
  validateSearch: (s: Record<string, unknown>): { page?: number } =>
    Number(s.page) > 0 ? { page: Math.floor(Number(s.page)) } : {},
  component: PaperPage,
});

const PdfViewer = lazy(() => import("@/features/materials/pdf-viewer").then((m) => ({ default: m.PdfViewer })));
const KEY_TONE = { Official: "success", Partial: "warning", Derived: "info", None: "neutral" } as const;

function PaperPage() {
  const { courseId, paperId } = Route.useParams();
  const { page } = Route.useSearch();
  const papers = usePapers(courseId);
  const paper = papers.data?.find((p) => p.id === paperId);
  const inBank = useQuestions(courseId, { paperId });
  const url = useQuery({
    queryKey: ["file", paper?.fileId, "url"],
    enabled: !!paper?.fileId,
    queryFn: () => fileUrl(paper!.fileId!),
    staleTime: 5 * 60_000,
  });
  if (papers.isPending) return <Skeleton className="h-96" />;
  if (!paper) return <EmptyList title="Not found, or you don't have access." />;
  const first = inBank.data?.pages[0];
  const n = first ? `${first.items.length}${first.nextCursor ? "+" : ""}` : "…";
  return (
    <div className="flex flex-col gap-4">
      <Link
        to="/c/$courseId/papers"
        params={{ courseId }}
        className="inline-flex items-center gap-1 self-start text-sm text-muted hover:text-text"
      >
        <ChevronLeft className="size-4" aria-hidden="true" />
        Past papers
      </Link>
      <div className="flex flex-col gap-2">
        <h1 className="m-0 text-xl font-semibold tracking-[-0.01em]">{paper.label}</h1>
        <div className="flex flex-wrap items-center gap-2 text-sm text-muted">
          <span className="font-mono text-xs">{paper.paperKey}</span>
          {paper.examDate ? <span>{paper.examDate}</span> : null}
          <Badge kind="status" tone={KEY_TONE[paper.keyStatus as keyof typeof KEY_TONE] ?? "neutral"}>
            Key: {paper.keyStatus}
          </Badge>
          <Link
            to="/c/$courseId/bank"
            params={{ courseId }}
            search={{ paperId }}
            className="underline-offset-4 hover:text-text hover:underline"
          >
            {n}
            {paper.totalQuestions ? ` of ${paper.totalQuestions}` : ""} questions in the bank
          </Link>
        </div>
        {paper.omissions.length ? (
          <ul className="m-0 flex flex-col gap-0.5 pl-5 text-sm text-muted">
            {paper.omissions.map((o) => (
              <li key={o.questionNumber}>
                Q{o.questionNumber} left out: {o.reason}
              </li>
            ))}
          </ul>
        ) : null}
        {paper.note ? <p className="m-0 text-sm">{paper.note}</p> : null}
      </div>
      {!paper.fileId ? (
        <Card>
          <p className="m-0 text-muted">No PDF for this paper. Add one with Edit on the Past papers page.</p>
        </Card>
      ) : url.data ? (
        <Suspense fallback={<Skeleton className="h-[70vh]" />}>
          <PdfViewer url={sameOrigin(url.data.url, window.location.origin)} initialPage={page ?? 1} />
        </Suspense>
      ) : url.isError ? (
        <Card>
          <p className="m-0 text-muted">This PDF isn&apos;t available (its original may have been removed).</p>
        </Card>
      ) : (
        <Skeleton className="h-[70vh]" />
      )}
    </div>
  );
}

import { useQuery } from "@tanstack/react-query";
import { createFileRoute, Link } from "@tanstack/react-router";
import { ChevronLeft } from "lucide-react";
import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { Markdown } from "@/components/markdown";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ProgressBar } from "@/components/ui/progress";
import { Banner } from "@/components/ui/banner";
import { EmptyList } from "@/components/ui/empty";
import { Skeleton } from "@/components/ui/misc";
import { fileUrl, getFile, pageText } from "@/features/files/api";
import { useCourse } from "@/features/courses/api";
import { canEdit } from "@/features/courses/lib/roles";
import { useMaterial } from "@/features/lectures/api";
import { useChecks, type MaterialCheck } from "@/features/practice/api";
import { CheckBlock, ManageChecksDialog, QuizDialog } from "@/features/practice/components";
import { afterSection, reached, readShare, splitSections } from "@/features/practice/lib/placement";
import { track } from "@/lib/activity";

// The material viewer (frontend-spec 4): pdf.js or markdown; a purged original shows its extracted
// text per page with a banner. Page views go to the activity queue. Check questions follow their
// page (PDF) or section (notes); without any, the end offers a quick quiz on the lecture. Ask AI
// arrives with step 8.
const PdfViewer = lazy(() => import("@/features/materials/pdf-viewer").then((m) => ({ default: m.PdfViewer })));

export const Route = createFileRoute("/_app/c/$courseId/m/$materialId")({
  validateSearch: (s: Record<string, unknown>): { page?: number } =>
    Number(s.page) > 0 ? { page: Number(s.page) } : {},
  component: Viewer,
});

function PurgedText({ fileId }: { fileId: string }) {
  const file = useQuery({ queryKey: ["file", fileId], queryFn: () => getFile(fileId) });
  const pages = file.data?.pageCount ?? 0;
  const texts = useQuery({
    queryKey: ["file", fileId, "texts"],
    enabled: pages > 0,
    queryFn: () => Promise.all(Array.from({ length: pages }, (_, i) => pageText(fileId, i + 1))),
  });
  if (texts.isPending) return <Skeleton className="h-96" />;
  return (
    <div className="flex flex-col gap-6">
      {(texts.data ?? []).map((t) => (
        <section key={t.page} className="flex flex-col gap-2">
          <h3 className="m-0 text-xs font-medium uppercase tracking-[0.06em] text-muted">Page {t.page}</h3>
          <p className="m-0 max-w-[72ch] whitespace-pre-wrap">{t.text || "(no text on this page)"}</p>
        </section>
      ))}
    </div>
  );
}

function Viewer() {
  const { courseId, materialId } = Route.useParams();
  const { page } = Route.useSearch();
  const found = useMaterial(courseId, materialId);
  const m = found.data?.material;
  const componentId = found.data?.lecture.id;
  const showPdf = !!m?.fileId && m.originalStatus !== "Purged";
  const url = useQuery({
    queryKey: ["file", m?.fileId, "url"],
    enabled: showPdf,
    queryFn: () => fileUrl(m!.fileId!),
    staleTime: 5 * 60_000,
  });
  const opened = useRef(Date.now());
  const [lastPage, setLastPage] = useState(0);
  const [maxPage, setMaxPage] = useState(0);
  const course = useCourse(courseId);
  const editor = canEdit(course.data?.myRole);
  const checks = useChecks(m?.id);
  const fileInfo = useQuery({
    queryKey: ["file", m?.fileId],
    enabled: showPdf,
    queryFn: () => getFile(m!.fileId!),
  });
  const pages = fileInfo.data?.pageCount ?? null;
  const [manage, setManage] = useState(false);
  const [quiz, setQuiz] = useState(false);

  // material.open now, material.close with the seconds spent (spec 5.11).
  useEffect(() => {
    if (!m || !componentId) return;
    opened.current = Date.now();
    track("material.open", { courseId, componentId, payload: { materialId: m.id } });
    return () =>
      track("material.close", {
        courseId,
        componentId,
        payload: { materialId: m.id, seconds: Math.round((Date.now() - opened.current) / 1000) },
      });
  }, [m?.id, componentId, courseId]); // eslint-disable-line react-hooks/exhaustive-deps

  const onPage = (n: number) => {
    if (n === lastPage || !m || !componentId) return;
    setLastPage(n);
    setMaxPage((x) => Math.max(x, n));
    track("material.page", { courseId, componentId, payload: { materialId: m.id, page: n } });
  };

  if (found.isPending) return <Skeleton className="h-96" />;
  if (!m || !found.data) return <EmptyList title="Not found, or you don't have access." />;
  return (
    <div className="flex flex-col gap-4">
      <Link
        to="/c/$courseId/l/$componentId"
        params={{ courseId, componentId: found.data.lecture.id }}
        className="inline-flex items-center gap-1 self-start text-sm text-muted hover:text-text"
      >
        <ChevronLeft className="size-4" aria-hidden="true" />
        {found.data.lecture.code} {found.data.lecture.name}
      </Link>
      <div className="flex flex-wrap items-center gap-2">
        <h2 className="m-0 mr-2 text-lg font-semibold">{m.title}</h2>
        {m.excluded ? <Badge tone="warning">Excluded</Badge> : null}
        {!m.aiAllowed ? <Badge>AI-blocked</Badge> : null}
        <span className="flex-1" />
        {editor ? (
          <Button size="sm" variant="ghost" onClick={() => setManage(true)}>
            Check questions{checks.data?.length ? ` (${checks.data.length})` : ""}
          </Button>
        ) : null}
      </div>
      {m.excluded && m.excludedNote ? <p className="m-0 text-sm text-muted">{m.excludedNote}</p> : null}
      {m.fileId && m.originalStatus === "Purged" ? (
        <>
          <Banner tone="notice">
            Original removed to save space. Re-upload the same PDF to restore it; your questions and notes stay
            connected.
          </Banner>
          <PurgedText fileId={m.fileId} />
        </>
      ) : m.fileId ? (
        url.data ? (
          <>
            {pages ? (
              <ProgressBar
                value={maxPage}
                max={pages}
                label="Read"
                detail={`page ${Math.min(maxPage, pages)} of ${pages}`}
                className={readShare(maxPage, pages) >= 1 ? "opacity-70" : undefined}
              />
            ) : null}
            <Suspense fallback={<Skeleton className="h-[70vh]" />}>
              <PdfViewer url={url.data.url} initialPage={page ?? m.pageFrom ?? 1} onPage={onPage} />
            </Suspense>
            <CheckBlock checks={reached(checks.data ?? [], maxPage, pages)} materialId={m.id} />
            {checks.data && !checks.data.length && pages && maxPage >= pages ? (
              <EndQuiz onQuiz={() => setQuiz(true)} />
            ) : null}
          </>
        ) : url.isError ? (
          <EmptyList
            title="This file isn't available right now."
            body="It may still be scanning. Try again in a moment."
          />
        ) : (
          <Skeleton className="h-[70vh]" />
        )
      ) : (
        <Notes content={m.content ?? ""} checks={checks.data ?? []} materialId={m.id} onQuiz={() => setQuiz(true)} />
      )}
      {manage && componentId ? (
        <ManageChecksDialog
          courseId={courseId}
          componentId={componentId}
          materialId={m.id}
          checks={checks.data ?? []}
          unit={m.fileId ? "page" : "section"}
          onClose={() => setManage(false)}
        />
      ) : null}
      {quiz && componentId ? (
        <QuizDialog
          courseId={courseId}
          componentId={componentId}
          title={`Check yourself: ${found.data.lecture.code} ${found.data.lecture.name}`}
          onClose={() => setQuiz(false)}
        />
      ) : null}
    </div>
  );
}

/** Notes in sections, each followed by its check questions; with none, a quiz offer at the end. */
function Notes({
  content,
  checks,
  materialId,
  onQuiz,
}: {
  content: string;
  checks: MaterialCheck[];
  materialId: string;
  onQuiz: () => void;
}) {
  const sections = splitSections(content);
  return (
    <div className="flex flex-col">
      {sections.map((sec, n) => (
        <div key={n}>
          {sec.trim() ? <Markdown>{sec}</Markdown> : null}
          <CheckBlock checks={afterSection(checks, n, sections.length)} materialId={materialId} />
        </div>
      ))}
      {!checks.length ? <EndQuiz onQuiz={onQuiz} /> : null}
    </div>
  );
}

function EndQuiz({ onQuiz }: { onQuiz: () => void }) {
  return (
    <div className="mt-4 flex flex-wrap items-center gap-3 rounded-lg border border-dashed border-[color:var(--course)] p-3">
      <span className="min-w-0 flex-1 text-sm">
        Finished? Check yourself with a couple of questions on this lecture.
      </span>
      <Button size="sm" variant="primary" onClick={onQuiz}>
        Quiz me
      </Button>
    </div>
  );
}

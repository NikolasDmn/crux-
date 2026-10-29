import { useQuery, useQueryClient } from "@tanstack/react-query";
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
import { useUser } from "@/app/auth";
import { SelectionAsk } from "@/features/ai/course-ask";
import { useToast } from "@/components/ui/toast";
import {
  fileUrl,
  getFile,
  markRestored,
  pageText,
  RESTORED,
  thumbnailUrl,
  useCancelFree,
  useUploads,
} from "@/features/files/api";
import { ACCEPT, phaseLabel, sameOrigin } from "@/features/files/lib/upload";
import type { Material } from "@/features/lectures/api";
import { canEdit, isOwner } from "@/features/courses/lib/roles";
import { messageFor } from "@/lib/errors";
import { formatInZone } from "@/lib/time";
import { useCourse } from "@/features/courses/api";
import { useMaterial } from "@/features/lectures/api";
import { useChecks, type MaterialCheck } from "@/features/practice/api";
import { CheckBlock, ManageChecksDialog, QuizDialog } from "@/features/practice/components";
import { afterSection, reached, readShare, splitSections } from "@/features/practice/lib/placement";
import { track } from "@/lib/activity";

// The material viewer (frontend-spec 4): pdf.js or markdown; a purged original shows its page
// thumbnails if kept, else its extracted text per page, with a banner and (Editors) Re-upload. Page views go to the activity queue. Check questions follow their
// page (PDF) or section (notes); without any, the end offers a quick quiz on the lecture. Selecting
// text in the PDF offers Ask AI about that quote.
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
  // Thumbnails exist only when the owner keeps them; a 404 on page 1 means text it is.
  const thumbs = useQuery({
    queryKey: ["file", fileId, "thumbnails"],
    enabled: pages > 0,
    retry: false,
    staleTime: 4 * 60_000,
    queryFn: async () => {
      const first = await thumbnailUrl(fileId, 1).catch(() => null);
      if (!first) return null;
      const rest = await Promise.all(
        Array.from({ length: pages - 1 }, (_, i) => thumbnailUrl(fileId, i + 2).catch(() => null)),
      );
      return [first, ...rest].map((t) => (t ? sameOrigin(t.url, location.origin) : null));
    },
  });
  if (texts.isPending || thumbs.isPending) return <Skeleton className="h-96" />;
  const textOf = new Map((texts.data ?? []).map((t) => [t.page, t.text]));
  if (thumbs.data)
    return (
      <div className="flex flex-col gap-6">
        {thumbs.data.map((src, i) => (
          <section key={i} className="flex flex-col gap-2">
            <h3 className="m-0 text-xs font-medium uppercase tracking-[0.06em] text-muted">Page {i + 1}</h3>
            {src ? (
              <img
                src={src}
                alt={`Page ${i + 1}: ${(textOf.get(i + 1) ?? "").slice(0, 200)}`}
                loading="lazy"
                className="w-full max-w-[640px] rounded-md border border-border bg-white"
              />
            ) : (
              <p className="m-0 max-w-[72ch] whitespace-pre-wrap">{textOf.get(i + 1) || "(no text on this page)"}</p>
            )}
          </section>
        ))}
      </div>
    );
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

/** Re-upload of a removed original (spec 5.1): only the same bytes bring it back, checked on the hash
 *  before anything is sent. */
function Reupload({ courseId, fileId }: { courseId: string; fileId: string }) {
  const qc = useQueryClient();
  const toast = useToast();
  const file = useQuery({ queryKey: ["file", fileId], queryFn: () => getFile(fileId) });
  const { uploads, add } = useUploads({
    courseId,
    check: (hash) =>
      file.data && hash !== file.data.sha256
        ? "That isn't the same PDF. Add a different file from the lecture page."
        : null,
    onClean: (f) => {
      markRestored(qc, f.id);
      void qc.invalidateQueries({ queryKey: ["file", fileId] });
      toast({ message: RESTORED });
    },
  });
  const last = uploads.at(-1);
  return (
    <span className="flex flex-wrap items-center gap-3">
      <label className="inline-flex h-8 cursor-pointer items-center rounded-md border border-border bg-surface px-3 text-sm hover:bg-sunken focus-within:outline-2 focus-within:outline-offset-2 focus-within:outline-focus">
        Re-upload
        <input
          type="file"
          accept={ACCEPT}
          className="sr-only"
          disabled={!file.data}
          onChange={(e) => {
            const f = e.currentTarget.files?.[0];
            if (f) add([f]);
            e.currentTarget.value = "";
          }}
        />
      </label>
      {last ? (
        <span
          className={last.state.phase === "failed" ? "text-sm text-danger-text" : "text-sm text-muted"}
          aria-live="polite"
        >
          {phaseLabel(last.state)}
        </span>
      ) : null}
    </span>
  );
}

/** A removal is pending: when it happens, and Undo for the owner. */
function PendingBanner({ m, owner }: { m: Material; owner: boolean }) {
  const user = useUser();
  const cancel = useCancelFree();
  const toast = useToast();
  return (
    <Banner tone="notice">
      <span className="flex flex-wrap items-center gap-3">
        <span className="flex-1">
          The original is removed to save space
          {m.originalPurgeAt ? ` on ${formatInZone(m.originalPurgeAt, user.timeZone, "d MMM")}` : " soon"}. Its text
          stays.
        </span>
        {owner ? (
          <Button
            size="sm"
            disabled={cancel.isPending}
            onClick={() => cancel.mutate(m.id, { onError: (e) => toast({ message: messageFor(e), tone: "danger" }) })}
          >
            Undo
          </Button>
        ) : null}
      </span>
    </Banner>
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
  const pagesRef = useRef<HTMLDivElement>(null);
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
            <span className="flex flex-wrap items-center gap-3">
              <span className="flex-1">
                Original removed to save space. Re-upload the same PDF to restore it; your questions and notes stay
                connected.
              </span>
              {editor ? <Reupload courseId={courseId} fileId={m.fileId} /> : null}
            </span>
          </Banner>
          <PurgedText fileId={m.fileId} />
        </>
      ) : m.fileId ? (
        url.data ? (
          <>
            {m.originalStatus === "PendingPurge" ? <PendingBanner m={m} owner={isOwner(course.data?.myRole)} /> : null}
            {pages ? (
              <ProgressBar
                value={maxPage}
                max={pages}
                label="Read"
                detail={`page ${Math.min(maxPage, pages)} of ${pages}`}
                className={readShare(maxPage, pages) >= 1 ? "opacity-70" : undefined}
              />
            ) : null}
            <div ref={pagesRef}>
              <Suspense fallback={<Skeleton className="h-[70vh]" />}>
                <PdfViewer url={url.data.url} initialPage={page ?? m.pageFrom ?? 1} onPage={onPage} />
              </Suspense>
            </div>
            {m.aiAllowed ? (
              <SelectionAsk root={pagesRef} courseId={courseId} materialId={m.id} fileId={m.fileId} title={m.title} />
            ) : null}
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

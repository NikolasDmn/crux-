import { useRouterState } from "@tanstack/react-router";
import { Sparkles } from "lucide-react";
import { useEffect, useRef, useState, type RefObject } from "react";
import { useCourse } from "@/features/courses/api";
import { useLectures, useMaterial } from "@/features/lectures/api";
import { AskButton, useAsk } from "./ask-panel";
import { useAi } from "./api";
import type { AskTarget } from "./lib/chat";

// Where Ask AI opens from (frontend-spec 4, 5.5): the course header anchors to the page's object
// (material, lecture, question, else the course); a text selection in the PDF viewer anchors to the
// selected quote and its page.

/** The course header's button: anchored to whatever the current page shows. */
export function CourseAskButton({ courseId }: { courseId: string }) {
  const params = useRouterState({
    select: (r) => Object.assign({}, ...r.matches.map((m) => m.params)) as Record<string, string | undefined>,
  });
  const course = useCourse(courseId);
  const lectures = useLectures(courseId);
  const material = useMaterial(courseId, params.materialId ?? "");
  let target: AskTarget;
  if (params.materialId) {
    target = {
      courseId,
      scope: "Material",
      materialId: params.materialId,
      title: material.data?.material.title ?? "This material",
    };
  } else if (params.componentId) {
    const l = lectures.data?.find((x) => x.id === params.componentId);
    target = {
      courseId,
      scope: "Component",
      componentId: params.componentId,
      title: l ? `${l.code} ${l.name}` : "This lecture",
    };
  } else if (params.questionId) {
    target = { courseId, scope: "Question", questionId: params.questionId, title: "This question" };
  } else {
    target = { courseId, scope: "Course", title: course.data?.code ?? "This course" };
  }
  return <AskButton target={target} />;
}

/** A floating "Ask AI" over a text selection inside `root` (the PDF viewer's pages). */
export function SelectionAsk({
  root,
  courseId,
  materialId,
  fileId,
  title,
}: {
  root: RefObject<HTMLElement | null>;
  courseId: string;
  materialId: string;
  fileId: string;
  title: string;
}) {
  const gate = useAi("MaterialChat", courseId);
  const { open } = useAsk();
  const [sel, setSel] = useState<{ x: number; y: number; quote: string; page: number } | null>(null);
  const last = useRef(sel);
  last.current = sel;

  useEffect(() => {
    if (!gate.allowed) return;
    const onUp = () => {
      const s = window.getSelection();
      const quote = s?.toString().trim() ?? "";
      const node = s?.anchorNode ?? null;
      const el = node instanceof Element ? node : node?.parentElement;
      const pageEl = el?.closest<HTMLElement>("[data-page]");
      if (!quote || !s?.rangeCount || !pageEl || !root.current?.contains(pageEl)) {
        setSel(null);
        return;
      }
      const r = s.getRangeAt(0).getBoundingClientRect();
      setSel({ x: r.left + r.width / 2, y: r.top, quote: quote.slice(0, 1000), page: Number(pageEl.dataset.page) });
    };
    const onScroll = () => last.current && setSel(null);
    document.addEventListener("mouseup", onUp);
    document.addEventListener("keyup", onUp);
    window.addEventListener("scroll", onScroll, true);
    return () => {
      document.removeEventListener("mouseup", onUp);
      document.removeEventListener("keyup", onUp);
      window.removeEventListener("scroll", onScroll, true);
    };
  }, [gate.allowed, root]);

  if (!sel) return null;
  return (
    <button
      type="button"
      // Keep the selection: a mousedown on the button would clear it first.
      onMouseDown={(e) => e.preventDefault()}
      onClick={() => {
        open({
          courseId,
          scope: "Material",
          materialId,
          title,
          anchor: { fileRef: fileId, page: sel.page, quote: sel.quote },
        });
        setSel(null);
      }}
      style={{ left: sel.x, top: Math.max(8, sel.y - 40) }}
      className="fixed z-30 inline-flex h-8 -translate-x-1/2 items-center gap-1.5 rounded-md bg-primary px-3 text-sm text-primary-text shadow-raised"
    >
      <Sparkles className="size-4" aria-hidden="true" />
      Ask AI
    </button>
  );
}

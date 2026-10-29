import { useQueries, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { Sparkles, ThumbsDown, ThumbsUp, X } from "lucide-react";
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Markdown } from "@/components/markdown";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Select, Textarea } from "@/components/ui/field";
import { Skeleton } from "@/components/ui/misc";
import { Tooltip } from "@/components/ui/overlays";
import { keys as lectureKeys, useLectures } from "@/features/lectures/api";
import { api, ApiError, unwrap } from "@/lib/api";
import { cn } from "@/lib/cn";
import { messageFor, retryAfter } from "@/lib/errors";
import {
  createConversation,
  sendMessage,
  useAi,
  useConversation,
  useConversations,
  useFeedback,
  useTutors,
  type Message,
} from "./api";
import { reasonText } from "./lib/format";
import {
  applyEvent,
  citedPage,
  emptyReply,
  linkCitations,
  sameTarget,
  titleFrom,
  type AskTarget,
  type Citation,
  type ContextUsed,
  type Reply,
} from "./lib/chat";

// The Ask AI side panel (frontend-spec 4, 5.5): a conversation anchored to the material, lecture,
// question, mock or course; a tutor picker; the reply streams through <Markdown> with `[p.N]`
// citation chips that open the viewer at that page; "Context used" under every reply; "Answered by"
// when a fallback model wrote it; thumbs up/down; past conversations on the same thing.

const Ctx = createContext<{ open: (t: AskTarget) => void } | null>(null);

export function useAsk() {
  const c = useContext(Ctx);
  if (!c) throw new Error("useAsk outside AskProvider");
  return c;
}

export function AskProvider({ children }: { children: ReactNode }) {
  const [target, setTarget] = useState<AskTarget | null>(null);
  const [n, setN] = useState(0);
  const open = useCallback((t: AskTarget) => {
    setTarget(t);
    setN((x) => x + 1);
  }, []);
  const value = useMemo(() => ({ open }), [open]);
  return (
    <Ctx.Provider value={value}>
      {children}
      {target ? <AskPanel key={n} target={target} onClose={() => setTarget(null)} /> : null}
    </Ctx.Provider>
  );
}

/** "Ask AI" for a page (2.5): hidden while AI is off or unreachable, disabled with the reason when
 *  blocked here. */
export function AskButton({ target, size = "sm" }: { target: AskTarget; size?: "sm" | "md" }) {
  const gate = useAi("MaterialChat", target.courseId);
  const { open } = useAsk();
  if (!gate.visible) return null;
  const why = gate.allowed ? null : reasonText(gate.reason, target.courseId);
  const button = (
    <Button size={size} disabled={!gate.allowed} onClick={() => open(target)}>
      <Sparkles aria-hidden="true" />
      Ask AI
    </Button>
  );
  return why ? (
    <Tooltip label={why.text}>
      <span tabIndex={0}>{button}</span>
    </Tooltip>
  ) : (
    button
  );
}

/** fileId → its material in this course, for citations and "Context used". */
function useCourseFiles(courseId: string) {
  const lectures = useLectures(courseId);
  const lists = useQueries({
    queries: (lectures.data ?? []).map((l) => ({
      queryKey: lectureKeys.materials(l.id),
      queryFn: async () =>
        (await unwrap(api.GET("/components/{id}/materials", { params: { path: { id: l.id } } }))).sort(
          (a, b) => a.sortOrder - b.sortOrder,
        ),
    })),
  });
  const map = new Map<string, { materialId: string; title: string }>();
  for (const l of lists)
    for (const m of l.data ?? []) if (m.fileId) map.set(m.fileId, { materialId: m.id, title: m.title });
  return map;
}

function AskPanel({ target, onClose }: { target: AskTarget; onClose: () => void }) {
  const qc = useQueryClient();
  const navigate = useNavigate();
  const gate = useAi("MaterialChat", target.courseId);
  const convs = useConversations(target.courseId, target.scope, target.materialId);
  const mine = useMemo(
    () =>
      (convs.data ?? []).filter((c) => sameTarget(c, target)).sort((a, b) => b.createdAt.localeCompare(a.createdAt)),
    [convs.data, target],
  );
  const [convId, setConvId] = useState<string | null | undefined>(undefined);
  // The latest conversation on this thing opens by default, once the list is in.
  useEffect(() => {
    if (convId === undefined && convs.data) setConvId(mine[0]?.id ?? null);
  }, [convId, convs.data, mine]);
  const conv = useConversation(convId ?? null);
  const tutors = useTutors();
  const [tutorId, setTutorId] = useState("");
  const [anchor, setAnchor] = useState(target.anchor);
  const [text, setText] = useState("");
  const [pending, setPending] = useState<{ question: string; reply: Reply } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [cooldown, setCooldown] = useState(0);
  const files = useCourseFiles(target.courseId);
  const end = useRef<HTMLDivElement>(null);
  const abort = useRef<AbortController | null>(null);
  const busy = !!pending && !pending.reply.done;

  useEffect(() => () => abort.current?.abort(), []);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  useEffect(() => {
    if (cooldown <= 0) return;
    const t = setTimeout(() => setCooldown((c) => c - 1), 1000);
    return () => clearTimeout(t);
  }, [cooldown]);
  // Braces: newer browsers return a Promise from scrollIntoView, which React would take as a cleanup.
  useEffect(() => {
    end.current?.scrollIntoView({ block: "end" });
  }, [conv.data?.messages?.length, pending?.reply.text]);

  /** Opens the viewer at a cited page of a file (or, without one, of the anchored material). */
  const openPage = (fileId: string | null, page: number) => {
    const m = fileId ? files.get(fileId) : undefined;
    const materialId = m?.materialId ?? (target.scope === "Material" ? target.materialId : undefined);
    if (!materialId) return;
    void navigate({
      to: "/c/$courseId/m/$materialId",
      params: { courseId: target.courseId, materialId },
      search: { page },
    });
    if (window.matchMedia("(max-width: 767px)").matches) onClose();
  };

  const send = async () => {
    const question = text.trim();
    if (!question || busy || cooldown > 0) return;
    setError(null);
    let id = convId ?? null;
    try {
      if (!id) {
        const c = await createConversation({
          courseId: target.courseId,
          scope: target.scope,
          ...(target.materialId ? { materialId: target.materialId } : {}),
          ...(target.componentId ? { componentId: target.componentId } : {}),
          ...(target.questionId ? { questionId: target.questionId } : {}),
          ...(target.mockId ? { mockId: target.mockId } : {}),
          ...(tutorId ? { tutorId } : {}),
          title: titleFrom(question),
        });
        id = c.id;
        qc.setQueryData(["ai", "conversation", id], { ...c, messages: [] });
        setConvId(id);
        void convs.refetch();
      }
      setText("");
      let reply = emptyReply();
      setPending({ question, reply });
      abort.current = new AbortController();
      for await (const ev of sendMessage(id, { content: question, anchor: anchor ?? null }, abort.current.signal)) {
        reply = applyEvent(reply, ev.event, ev.data);
        setPending({ question, reply });
      }
      setAnchor(undefined);
      if (!reply.error) {
        await qc.invalidateQueries({ queryKey: ["ai", "conversation", id] });
        setPending(null);
      } else {
        setPending({ question, reply: { ...reply, done: true } });
      }
    } catch (e) {
      if (e instanceof DOMException && e.name === "AbortError") return;
      setPending(null);
      if (e instanceof ApiError && e.code === "rate_limited") {
        setText(question);
        setCooldown(retryAfter(e) ?? 60);
      } else {
        setText(question);
        setError(messageFor(e));
      }
    }
  };

  const stored = conv.data?.messages ?? [];
  const cites =
    (msg: { citations: Citation[]; contextUsed?: ContextUsed | null }) => (href?: string, children?: ReactNode) => {
      const page = citedPage(href);
      if (page === null) return undefined;
      const fileId =
        msg.citations.find((c) => c.page === page)?.fileId ??
        msg.contextUsed?.pages?.find((p) => p.page === page)?.fileId ??
        null;
      return (
        <button
          type="button"
          onClick={() => openPage(fileId, page)}
          className="mx-0.5 inline-flex h-5 items-center rounded bg-sunken px-1.5 align-baseline font-mono text-xs text-text no-underline hover:bg-[color:var(--course-soft,var(--sunken))]"
        >
          {children}
        </button>
      );
    };

  return (
    <aside
      aria-label="Ask AI"
      className="fixed inset-0 z-40 flex flex-col bg-bg md:inset-y-0 md:left-auto md:right-0 md:w-[440px] md:border-l md:border-border md:shadow-raised"
    >
      <header className="flex flex-col gap-2 border-b border-border px-4 py-3">
        <div className="flex items-center gap-2">
          <Sparkles className="size-4 text-muted" aria-hidden="true" />
          <h2 className="m-0 min-w-0 flex-1 truncate text-base font-semibold">Ask AI · {target.title}</h2>
          <Button size="iconSm" variant="ghost" aria-label="Close" onClick={onClose}>
            <X />
          </Button>
        </div>
        <div className="flex gap-2">
          <Select
            aria-label="Conversation"
            className="h-8 min-w-0 flex-1 text-sm"
            value={convId ?? ""}
            disabled={busy}
            onChange={(e) => {
              setPending(null);
              setConvId(e.currentTarget.value || null);
            }}
          >
            <option value="">New conversation</option>
            {mine.map((c) => (
              <option key={c.id} value={c.id}>
                {c.title || "Untitled"} · {c.createdAt.slice(0, 10)}
              </option>
            ))}
          </Select>
          <Select
            aria-label="Tutor"
            className="h-8 w-40 text-sm"
            value={tutorId}
            disabled={busy}
            onChange={(e) => {
              setTutorId(e.currentTarget.value);
              // The tutor is fixed per conversation: another tutor starts a new one.
              if (stored.length) setConvId(null);
            }}
          >
            <option value="">Default tutor</option>
            {(tutors.data ?? []).map((t) => (
              <option key={t.id} value={t.id}>
                {t.name}
              </option>
            ))}
          </Select>
        </div>
      </header>

      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-4 py-4" aria-live="polite">
        {!gate.loading && !gate.allowed ? (
          <Banner tone="notice">{reasonText(gate.reason, target.courseId)?.text ?? "AI isn't available here."}</Banner>
        ) : null}
        {convId && conv.isPending ? <Skeleton className="h-24" /> : null}
        {!stored.length && !pending && !conv.isPending ? (
          <p className="m-0 text-sm text-muted">
            Ask about {target.title}. Answers come from your course materials and cite their pages.
          </p>
        ) : null}
        {stored.map((m) =>
          m.role === "User" ? (
            <UserBubble key={m.id} text={m.content} anchor={m.anchor} />
          ) : (
            <AssistantMessage
              key={m.id}
              reply={stored2reply(m)}
              conversationId={convId ?? null}
              feedback={m.feedback}
              link={cites({ citations: m.citations, contextUsed: m.contextUsed as ContextUsed | null })}
              files={files}
              onPage={openPage}
            />
          ),
        )}
        {pending ? (
          <>
            <UserBubble text={pending.question} anchor={null} />
            <AssistantMessage
              reply={pending.reply}
              conversationId={null}
              feedback={null}
              link={cites(pending.reply)}
              files={files}
              onPage={openPage}
            />
          </>
        ) : null}
        <div ref={end} />
      </div>

      <form
        className="flex flex-col gap-2 border-t border-border px-4 py-3"
        onSubmit={(e) => {
          e.preventDefault();
          void send();
        }}
      >
        {anchor && "quote" in anchor ? (
          <div className="flex items-start gap-2 rounded-md bg-sunken px-3 py-2 text-sm">
            <span className="min-w-0 flex-1">
              <span className="text-muted">About p.{anchor.page}: </span>“{anchor.quote.slice(0, 200)}”
            </span>
            <button type="button" aria-label="Don't send the selection" onClick={() => setAnchor(undefined)}>
              <X className="size-4 text-muted" />
            </button>
          </div>
        ) : null}
        {error ? <p className="m-0 text-sm text-danger-text">{error}</p> : null}
        {cooldown > 0 ? (
          <p className="m-0 text-sm text-muted" role="status">
            That's a lot of questions in a minute. You can ask again in {cooldown} s.
          </p>
        ) : null}
        <Textarea
          aria-label="Your question"
          rows={3}
          maxLength={8000}
          value={text}
          placeholder="Ask a question. Enter sends, Shift+Enter for a new line."
          onChange={(e) => setText(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
              e.preventDefault();
              void send();
            }
          }}
        />
        <div className="flex items-center justify-end gap-2">
          {busy ? (
            <Button type="button" size="sm" onClick={() => abort.current?.abort()}>
              Stop
            </Button>
          ) : null}
          <Button
            type="submit"
            size="sm"
            variant="primary"
            disabled={!gate.allowed || busy || cooldown > 0 || !text.trim()}
          >
            Send
          </Button>
        </div>
      </form>
    </aside>
  );
}

function stored2reply(m: Message): Reply {
  return {
    ...emptyReply(),
    text: m.content,
    citations: m.citations,
    messageId: m.id,
    servedBy: m.servedBy ?? null,
    contextUsed: (m.contextUsed as ContextUsed | null | undefined) ?? null,
    done: true,
  };
}

function UserBubble({ text, anchor }: { text: string; anchor: Record<string, unknown> | null | undefined }) {
  const quote = typeof anchor?.quote === "string" ? anchor.quote : null;
  return (
    <div className="flex flex-col items-end gap-1">
      {quote ? <span className="max-w-[90%] truncate text-xs text-muted">“{quote}”</span> : null}
      <p className="m-0 max-w-[90%] whitespace-pre-wrap rounded-lg bg-sunken px-3 py-2">{text}</p>
    </div>
  );
}

function AssistantMessage({
  reply,
  conversationId,
  feedback,
  link,
  files,
  onPage,
}: {
  reply: Reply;
  conversationId: string | null;
  feedback: string | null;
  link: (href?: string, children?: ReactNode) => ReactNode | undefined;
  files: Map<string, { materialId: string; title: string }>;
  onPage: (fileId: string | null, page: number) => void;
}) {
  const rate = useFeedback(conversationId);
  if (!reply.text && !reply.done) return <p className="m-0 text-sm text-muted">Thinking…</p>;
  return (
    <div className="flex flex-col gap-2">
      {reply.text ? <Markdown link={(h, c) => link(h, c)}>{linkCitations(reply.text)}</Markdown> : null}
      {reply.error ? (
        <p className="m-0 text-sm text-danger-text">
          {reply.error.code === "provider_auth"
            ? "Your AI provider rejected the key. Check it in Settings > AI."
            : reply.error.code === "ai_budget_exceeded"
              ? "This month's AI spending cap is reached."
              : `The AI provider failed: ${reply.error.message}`}
        </p>
      ) : null}
      {reply.servedBy ? <p className="m-0 text-xs text-muted">Answered by {reply.servedBy}</p> : null}
      {reply.done && reply.contextUsed ? <ContextList used={reply.contextUsed} files={files} onPage={onPage} /> : null}
      {reply.messageId !== null && conversationId ? (
        <div className="flex gap-1">
          {(["Up", "Down"] as const).map((f) => (
            <Button
              key={f}
              size="iconSm"
              variant="ghost"
              aria-label={f === "Up" ? "Helpful" : "Not helpful"}
              aria-pressed={feedback === f}
              className={cn(feedback === f && "bg-sunken")}
              onClick={() => rate.mutate({ messageId: reply.messageId!, feedback: feedback === f ? null : f })}
            >
              {f === "Up" ? <ThumbsUp /> : <ThumbsDown />}
            </Button>
          ))}
        </div>
      ) : null}
    </div>
  );
}

const SHARED: Record<string, string> = { progress: "progress", mistakes: "mistakes", answers: "answers" };

/** "Context used" (5.5): what the model saw. A requirement, not a nice-to-have. */
function ContextList({
  used,
  files,
  onPage,
}: {
  used: ContextUsed;
  files: Map<string, { materialId: string; title: string }>;
  onPage: (fileId: string | null, page: number) => void;
}) {
  const mistakes = [...(used.weakSpots ?? []), ...(used.slips ?? []), ...(used.traps ?? [])];
  const shared = (used.shared ?? []).map((s) => SHARED[s] ?? s);
  return (
    <details className="rounded-md border border-border text-sm">
      <summary className="cursor-pointer px-3 py-1.5 text-muted">Context used</summary>
      <div className="flex flex-col gap-2 px-3 pb-3 pt-1">
        {used.pages?.length ? (
          <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
            {used.pages.map((p, i) => (
              <li key={i}>
                <button
                  type="button"
                  className="text-left underline-offset-4 hover:underline"
                  onClick={() => p.page && onPage(p.fileId, p.page)}
                >
                  {files.get(p.fileId)?.title ?? "A material"}
                  {p.page ? ` p.${p.page}` : ""}
                </button>
                <span className="block truncate text-muted">“{p.quote}”</span>
              </li>
            ))}
          </ul>
        ) : (
          <p className="m-0 text-muted">No material pages matched.</p>
        )}
        {mistakes.length ? (
          <p className="m-0">
            <span className="text-muted">Your mistakes: </span>
            {mistakes
              .map((m) => m.label ?? "")
              .filter(Boolean)
              .join("; ")}
          </p>
        ) : null}
        {used.questionId ? <p className="m-0 text-muted">The question and your answer.</p> : null}
        <p className="m-0 text-muted">
          {shared.length ? `Shared about you: ${shared.join(", ")}.` : "Nothing about you was shared."}
        </p>
      </div>
    </details>
  );
}

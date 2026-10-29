import { createFileRoute, Link, useNavigate } from "@tanstack/react-router";
import { Download, MoreHorizontal, Plus, Search } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { EmptyList } from "@/components/ui/empty";
import { Input, Select } from "@/components/ui/field";
import { Skeleton } from "@/components/ui/misc";
import { Menu, MenuContent, MenuItem, MenuTrigger } from "@/components/ui/overlays";
import { useToast } from "@/components/ui/toast";
import { useCourse } from "@/features/courses/api";
import { canEdit } from "@/features/courses/lib/roles";
import { usePapers, useQuestionAction, useQuestions, type Question } from "@/features/bank/api";
import {
  activeCount,
  download,
  plainLine,
  readFilters,
  toJson,
  toMarkdown,
  type BankFilters,
} from "@/features/bank/lib/filters";
import { useLectures } from "@/features/lectures/api";
import { GenerateButton } from "@/features/ai/generate";
import { messageFor } from "@/lib/errors";
import { useVirtualRows } from "@/components/ui/virtual";
import { PromptLine } from "@/features/bank/components/prompt-line";

// Question bank (frontend-spec 4): a table with filters (lecture, type, source, paper, key
// reliability, review state, untested) and search, kept in the URL; badges for Generated, Draft,
// Disputed and Private; bulk archive; export of the filtered list (Markdown/JSON, in the browser).
// "Generate questions" (AI gate) writes new ones for a lecture.
export const Route = createFileRoute("/_app/c/$courseId/bank/")({
  validateSearch: (s: Record<string, unknown>): BankFilters => readFilters(s),
  component: Bank,
});

function Bank() {
  const { courseId } = Route.useParams();
  const f = Route.useSearch();
  const navigate = useNavigate({ from: Route.fullPath });
  const course = useCourse(courseId);
  const editor = canEdit(course.data?.myRole);
  const lectures = useLectures(courseId);
  const papers = usePapers(courseId);
  const list = useQuestions(courseId, f);
  const action = useQuestionAction();
  const toast = useToast();
  const [q, setQ] = useState(f.q ?? "");
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const set = (patch: Partial<BankFilters>) =>
    void navigate({ search: (old: BankFilters) => ({ ...old, ...patch }), replace: true });

  // Search follows typing, 300 ms behind.
  useEffect(() => {
    const t = setTimeout(() => (q.trim() || undefined) !== f.q && set({ q: q.trim() || undefined }), 300);
    return () => clearTimeout(t);
  }, [q]); // eslint-disable-line react-hooks/exhaustive-deps

  const rows = useMemo(() => (list.data?.pages ?? []).flatMap((p) => p.items as Question[]), [list.data]);
  const virt = useVirtualRows<HTMLTableSectionElement>(rows.length, 33);
  const code = useMemo(() => {
    const m = new Map((lectures.data ?? []).map((l) => [l.id, l.code]));
    return (id: string) => m.get(id) ?? "?";
  }, [lectures.data]);

  const exportAs = (kind: "md" | "json") => {
    const name = `${course.data?.code ?? "questions"}-${new Date().toISOString().slice(0, 10)}`;
    if (kind === "md") download(`${name}.md`, toMarkdown(rows, code), "text/markdown");
    else download(`${name}.json`, toJson(rows), "application/json");
  };

  const archive = async () => {
    for (const id of picked) {
      try {
        await action.mutateAsync({ id, action: "archive" });
      } catch (e) {
        toast({ message: messageFor(e), tone: "danger" });
        return;
      }
    }
    toast({ message: `${picked.size} archived` });
    setPicked(new Set());
  };

  const toggle = (id: string) =>
    setPicked((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center gap-2">
        <div className="relative min-w-[220px] flex-1">
          <Search className="absolute left-2.5 top-2.5 size-4 text-muted" aria-hidden="true" />
          <Input
            aria-label="Search questions"
            placeholder="Search prompts"
            className="pl-8"
            value={q}
            onChange={(e) => setQ(e.currentTarget.value)}
          />
        </div>
        <Menu>
          <MenuTrigger asChild>
            <Button size="sm" disabled={!rows.length}>
              <Download className="size-4" aria-hidden="true" />
              Export
            </Button>
          </MenuTrigger>
          <MenuContent align="end">
            <MenuItem onSelect={() => exportAs("md")}>Markdown ({rows.length})</MenuItem>
            <MenuItem onSelect={() => exportAs("json")}>JSON ({rows.length})</MenuItem>
          </MenuContent>
        </Menu>
        <GenerateButton courseId={courseId} componentId={f.componentId} />
        <Button asChild size="sm" variant="primary">
          <Link to="/c/$courseId/bank/new" params={{ courseId }}>
            <Plus className="size-4" aria-hidden="true" />
            New question
          </Link>
        </Button>
      </div>
      <div className="flex flex-wrap items-center gap-2 text-sm">
        <Filter label="Lecture" value={f.componentId} onChange={(v) => set({ componentId: v })}>
          {(lectures.data ?? []).map((l) => (
            <option key={l.id} value={l.id}>
              {l.code} {l.name}
            </option>
          ))}
        </Filter>
        <Filter label="Type" value={f.type} onChange={(v) => set({ type: v })}>
          {["MC", "Numeric", "Open"].map((t) => (
            <option key={t}>{t}</option>
          ))}
        </Filter>
        <Filter label="Source" value={f.source} onChange={(v) => set({ source: v })}>
          {["Manual", "Lecture", "Tutorial", "PastExam", "Practice", "Imported", "Generated", "Unknown"].map((t) => (
            <option key={t}>{t}</option>
          ))}
        </Filter>
        <Filter label="Paper" value={f.paperId} onChange={(v) => set({ paperId: v })}>
          {(papers.data ?? []).map((p) => (
            <option key={p.id} value={p.id}>
              {p.label}
            </option>
          ))}
        </Filter>
        <Filter label="Key" value={f.keyReliability} onChange={(v) => set({ keyReliability: v })}>
          {["Official", "Derived", "Disputed"].map((t) => (
            <option key={t}>{t}</option>
          ))}
        </Filter>
        <Filter label="State" value={f.reviewState} onChange={(v) => set({ reviewState: v })}>
          {["Approved", "Draft"].map((t) => (
            <option key={t}>{t}</option>
          ))}
        </Filter>
        <Check label="Untested" checked={!!f.untested} onChange={(v) => set({ untested: v || undefined })} />
        <Check
          label="Archived too"
          checked={!!f.includeArchived}
          onChange={(v) => set({ includeArchived: v || undefined })}
        />
        {activeCount(f) ? (
          <Button size="sm" variant="ghost" onClick={() => void navigate({ search: {}, replace: true })}>
            Clear filters
          </Button>
        ) : null}
      </div>
      {picked.size && editor ? (
        <Banner
          tone="notice"
          action={
            <div className="flex gap-2">
              <Button size="sm" onClick={() => setPicked(new Set())}>
                Clear
              </Button>
              <Button size="sm" variant="destructive" disabled={action.isPending} onClick={() => void archive()}>
                Archive {picked.size}
              </Button>
            </div>
          }
        >
          {picked.size} selected
        </Banner>
      ) : null}
      {list.isPending ? (
        <Skeleton className="h-64" />
      ) : list.isError ? (
        <Banner tone="danger">{messageFor(list.error)}</Banner>
      ) : !rows.length ? (
        <EmptyList
          title={activeCount(f) || f.q ? "No questions match" : "No questions yet"}
          body={
            activeCount(f) || f.q
              ? "Try fewer filters."
              : "Add questions by hand, or from past papers. They feed mocks, warm-ups and practice."
          }
          action={
            <Button asChild size="sm" variant="primary">
              <Link to="/c/$courseId/bank/new" params={{ courseId }}>
                New question
              </Link>
            </Button>
          }
        />
      ) : (
        <Card className="overflow-x-auto p-0">
          <table className="w-full min-w-[720px] border-collapse text-sm">
            <thead className="sticky top-0 bg-surface">
              <tr className="text-left text-xs uppercase tracking-[0.06em] text-muted">
                {editor ? <th className="w-10 px-3 py-2" /> : null}
                <th className="w-14 py-2 pl-3 font-medium">Lecture</th>
                <th className="w-20 py-2 font-medium">Type</th>
                <th className="py-2 font-medium">Prompt</th>
                <th className="w-28 py-2 font-medium">Source</th>
                <th className="w-40 py-2 font-medium" />
                <th className="w-10 py-2 pr-2" />
              </tr>
            </thead>
            <tbody ref={virt.ref}>
              {virt.before ? (
                <tr aria-hidden="true" style={{ height: virt.before }}>
                  <td colSpan={editor ? 7 : 6} />
                </tr>
              ) : null}
              {virt.indices
                .map((i) => rows[i]!)
                .map((r, n) => (
                  <tr
                    key={r.id}
                    ref={virt.measure}
                    data-index={virt.indices[n]}
                    className="h-8 border-t border-border hover:bg-sunken"
                  >
                    {editor ? (
                      <td className="px-3">
                        <input
                          type="checkbox"
                          aria-label={`Select question ${plainLine(r.current.prompt, 40)}`}
                          className="size-4 accent-[var(--primary)] align-middle"
                          checked={picked.has(r.id)}
                          onChange={() => toggle(r.id)}
                        />
                      </td>
                    ) : null}
                    <td className="pl-3 font-mono text-xs">{code(r.componentId)}</td>
                    <td className="text-muted">{r.type}</td>
                    <td className="max-w-0 truncate pr-3">
                      <Link
                        to="/c/$courseId/q/$questionId"
                        params={{ courseId, questionId: r.id }}
                        className={r.archivedAt ? "text-muted line-through" : "hover:underline"}
                      >
                        <PromptLine md={r.current.prompt} empty="(no prompt)" />
                      </Link>
                    </td>
                    <td className="truncate text-muted">
                      {r.paperQuestionNo ? `Q${r.paperQuestionNo}` : (r.sourceLabel ?? r.source)}
                    </td>
                    <td className="pr-3">
                      <span className="flex flex-wrap justify-end gap-1">
                        {r.source === "Generated" ? (
                          <Badge kind="status" tone="info">
                            Generated
                          </Badge>
                        ) : null}
                        {r.reviewState === "Draft" ? (
                          <Badge kind="status" tone="warning">
                            Draft
                          </Badge>
                        ) : null}
                        {r.current.keyReliability === "Disputed" ? (
                          <Badge kind="status" tone="danger">
                            Disputed
                          </Badge>
                        ) : null}
                        {r.private ? <Badge kind="status">Private</Badge> : null}
                      </span>
                    </td>
                    <td className="pr-2">
                      {editor || r.private ? (
                        <Menu>
                          <MenuTrigger asChild>
                            <Button size="iconSm" variant="ghost" aria-label="Question actions">
                              <MoreHorizontal className="size-4" />
                            </Button>
                          </MenuTrigger>
                          <MenuContent align="end">
                            <MenuItem
                              onSelect={() =>
                                void navigate({
                                  to: "/c/$courseId/q/$questionId/edit",
                                  params: { courseId, questionId: r.id },
                                })
                              }
                            >
                              Edit
                            </MenuItem>
                            {r.reviewState === "Draft" ? (
                              <MenuItem
                                onSelect={() =>
                                  action.mutate(
                                    { id: r.id, action: "approve" },
                                    { onError: (e) => toast({ message: messageFor(e), tone: "danger" }) },
                                  )
                                }
                              >
                                Approve
                              </MenuItem>
                            ) : null}
                            {!r.archivedAt ? (
                              <MenuItem
                                onSelect={() =>
                                  action.mutate(
                                    { id: r.id, action: "archive" },
                                    { onError: (e) => toast({ message: messageFor(e), tone: "danger" }) },
                                  )
                                }
                              >
                                Archive
                              </MenuItem>
                            ) : null}
                          </MenuContent>
                        </Menu>
                      ) : null}
                    </td>
                  </tr>
                ))}
              {virt.after ? (
                <tr aria-hidden="true" style={{ height: virt.after }}>
                  <td colSpan={editor ? 7 : 6} />
                </tr>
              ) : null}
            </tbody>
          </table>
          {list.hasNextPage ? (
            <div className="border-t border-border p-3 text-center">
              <Button size="sm" disabled={list.isFetchingNextPage} onClick={() => void list.fetchNextPage()}>
                Load more
              </Button>
            </div>
          ) : null}
        </Card>
      )}
    </div>
  );
}

function Filter({
  label,
  value,
  onChange,
  children,
}: {
  label: string;
  value: string | undefined;
  onChange: (v: string | undefined) => void;
  children: React.ReactNode;
}) {
  return (
    <Select
      aria-label={label}
      className="h-8 w-auto text-sm"
      value={value ?? ""}
      onChange={(e) => onChange(e.currentTarget.value || undefined)}
    >
      <option value="">{label}: any</option>
      {children}
    </Select>
  );
}

function Check({ label, checked, onChange }: { label: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <label className="flex h-8 items-center gap-1.5 px-1">
      <input
        type="checkbox"
        className="size-4 accent-[var(--primary)]"
        checked={checked}
        onChange={(e) => onChange(e.currentTarget.checked)}
      />
      {label}
    </label>
  );
}

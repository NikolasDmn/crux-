import { createFileRoute, Link, useNavigate } from "@tanstack/react-router";
import { Search } from "lucide-react";
import { useState, type FormEvent } from "react";
import { Markdown } from "@/components/markdown";
import { Badge } from "@/components/ui/badge";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Card, SectionLabel } from "@/components/ui/card";
import { EmptyList } from "@/components/ui/empty";
import { Field, Input, Select } from "@/components/ui/field";
import { Dot, Skeleton } from "@/components/ui/misc";
import { Dialog } from "@/components/ui/overlays";
import { useCourses } from "@/features/courses/api";
import { findByCode, useQueue, type Card as FlashCard } from "@/features/flash/api";
import { useLectures } from "@/features/lectures/api";
import { messageFor } from "@/lib/errors";

// Flashcards (frontend-spec 4): today's queue (due + new) across courses and per course; a custom
// set (lectures; from slips, forgotten, unseen or all; count); lookup by card code (BSA017).
export const Route = createFileRoute("/_app/flash/")({ component: Flash });

function Flash() {
  const queue = useQueue();
  const courses = useCourses();
  const live = (courses.data ?? []).filter((c) => !c.trashedAt && c.mySubscription === "Active");
  const cards = queue.data?.cards ?? [];
  const per = (id: string) => {
    const mine = cards.filter((c) => c.courseId === id);
    return {
      due: mine.filter((c) => c.progress.state !== "New").length,
      fresh: mine.filter((c) => c.progress.state === "New").length,
    };
  };

  return (
    <div className="flex max-w-[860px] flex-col gap-5">
      <div className="flex flex-wrap items-center gap-3">
        <h1 className="m-0 mr-auto text-xl font-semibold tracking-[-0.01em]">Flashcards</h1>
        <Lookup />
      </div>
      {queue.isPending ? (
        <Skeleton className="h-40" />
      ) : queue.isError ? (
        <Banner tone="danger">{messageFor(queue.error)}</Banner>
      ) : (
        <Card className="flex flex-wrap items-center gap-6">
          <div className="flex flex-col">
            <span className="font-mono text-3xl font-semibold tnum">{queue.data.dueCount}</span>
            <span className="text-sm text-muted">due</span>
          </div>
          <div className="flex flex-col">
            <span className="font-mono text-3xl font-semibold tnum">{queue.data.newCount}</span>
            <span className="text-sm text-muted">new today</span>
          </div>
          <span className="flex-1" />
          {cards.length ? (
            <Button asChild variant="primary" size="lg">
              <Link to="/flash/review">Review {cards.length}</Link>
            </Button>
          ) : (
            <Button variant="primary" size="lg" disabled>
              Nothing due
            </Button>
          )}
        </Card>
      )}
      {live.length ? (
        <section className="flex flex-col gap-3">
          <SectionLabel>By course</SectionLabel>
          <Card className="p-0">
            <ul className="m-0 flex list-none flex-col divide-y divide-border p-0">
              {live.map((c) => {
                const n = per(c.id);
                return (
                  <li key={c.id} className="flex items-center gap-3 px-5 py-3">
                    <Dot color={c.color} />
                    <Link to="/c/$courseId/cards" params={{ courseId: c.id }} className="font-medium hover:underline">
                      {c.code}
                    </Link>
                    <span className="min-w-0 flex-1 truncate text-sm text-muted">{c.name}</span>
                    <span className="font-mono text-sm tnum">
                      {n.due} due · {n.fresh} new
                    </span>
                    {n.due || n.fresh ? (
                      <Button asChild size="sm">
                        <Link to="/flash/review" search={{ courseId: c.id }}>
                          Review
                        </Link>
                      </Button>
                    ) : (
                      <Button size="sm" disabled>
                        Review
                      </Button>
                    )}
                  </li>
                );
              })}
            </ul>
          </Card>
        </section>
      ) : (
        <EmptyList title="No courses yet" body="Flashcards belong to a course's lectures." />
      )}
      {live.length ? <CustomSet courses={live.map((c) => ({ id: c.id, code: c.code }))} /> : null}
    </div>
  );
}

function CustomSet({ courses }: { courses: { id: string; code: string }[] }) {
  const navigate = useNavigate();
  const [courseId, setCourseId] = useState(courses[0]?.id ?? "");
  const lectures = useLectures(courseId);
  const [picked, setPicked] = useState<string[]>([]);
  const [which, setWhich] = useState<"slips" | "forgotten" | "unseen" | "all">("forgotten");
  const [n, setN] = useState("30");
  const submit = (e: FormEvent) => {
    e.preventDefault();
    void navigate({
      to: "/flash/review",
      search: { courseId, which, n: Number(n) || 30, ...(picked.length ? { lectures: picked.join(",") } : {}) },
    });
  };
  return (
    <Card className="flex flex-col gap-4">
      <SectionLabel>Custom set</SectionLabel>
      <p className="m-0 text-sm text-muted">Outside today's schedule; your ratings still count.</p>
      <form className="flex flex-col gap-4" onSubmit={submit}>
        <div className="grid gap-4 sm:grid-cols-3">
          <Field label="Course">
            {(ids) => (
              <Select
                {...ids}
                value={courseId}
                onChange={(e) => {
                  setCourseId(e.currentTarget.value);
                  setPicked([]);
                }}
              >
                {courses.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.code}
                  </option>
                ))}
              </Select>
            )}
          </Field>
          <Field label="Cards">
            {(ids) => (
              <Select {...ids} value={which} onChange={(e) => setWhich(e.currentTarget.value as typeof which)}>
                <option value="forgotten">Forgotten at least once</option>
                <option value="slips">Made from my slips</option>
                <option value="unseen">Not seen yet</option>
                <option value="all">All</option>
              </Select>
            )}
          </Field>
          <Field label="How many">
            {(ids) => (
              <Select {...ids} value={n} onChange={(e) => setN(e.currentTarget.value)}>
                {["10", "20", "30", "50", "100"].map((x) => (
                  <option key={x}>{x}</option>
                ))}
              </Select>
            )}
          </Field>
        </div>
        {lectures.data?.length ? (
          <div className="flex flex-col gap-1.5">
            <span className="text-sm font-medium">Lectures</span>
            <div className="flex flex-wrap gap-1.5">
              {lectures.data.map((l) => {
                const on = picked.includes(l.id);
                return (
                  <button
                    key={l.id}
                    type="button"
                    aria-pressed={on}
                    className={
                      on
                        ? "h-8 rounded-md border border-text bg-sunken px-2.5 text-sm font-medium"
                        : "h-8 rounded-md border border-border px-2.5 text-sm text-muted"
                    }
                    onClick={() => setPicked(on ? picked.filter((x) => x !== l.id) : [...picked, l.id])}
                  >
                    {l.code}
                  </button>
                );
              })}
            </div>
            <span className="text-xs text-muted">None picked: all lectures.</span>
          </div>
        ) : null}
        <div>
          <Button type="submit" disabled={!courseId}>
            Start custom set
          </Button>
        </div>
      </form>
    </Card>
  );
}

/** Find a card by its code (BSA017), e.g. from a printed sheet or a classmate. */
function Lookup() {
  const [code, setCode] = useState("");
  const [found, setFound] = useState<FlashCard[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    try {
      const r = await findByCode(code.trim().toUpperCase());
      if (!r.length) setError(`No card ${code.trim().toUpperCase()} in your courses.`);
      else setFound(r);
    } catch (err) {
      setError(messageFor(err));
    }
  };
  return (
    <form className="flex items-center gap-2" onSubmit={(e) => void submit(e)}>
      <div className="relative">
        <Search className="absolute left-2.5 top-2.5 size-4 text-muted" aria-hidden="true" />
        <Input
          aria-label="Find a card by code"
          placeholder="BSA017"
          className="w-36 pl-8 font-mono uppercase"
          value={code}
          onChange={(e) => setCode(e.currentTarget.value)}
        />
      </div>
      <Button size="sm" type="submit" disabled={!code.trim()}>
        Find
      </Button>
      {error ? <span className="text-sm text-danger-text">{error}</span> : null}
      <Dialog open={!!found} onOpenChange={(o) => !o && setFound(null)} title={found?.[0]?.code ?? ""}>
        {found?.map((c) => (
          <div key={c.id} className="flex flex-col gap-3">
            {c.personal ? <Badge kind="status">Personal</Badge> : null}
            <Markdown>{c.front}</Markdown>
            <div className="border-t border-border pt-3">
              <Markdown>{c.back}</Markdown>
            </div>
          </div>
        ))}
      </Dialog>
    </form>
  );
}

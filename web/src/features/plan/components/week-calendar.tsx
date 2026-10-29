import type { TZDate } from "@date-fns/tz";
import { Flag } from "lucide-react";
import { useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { useUser } from "@/app/auth";
import { PlanBlock } from "@/components/ui/plan-block";
import { EntryLabel } from "@/features/plan/components/entry-label";
import { useToast } from "@/components/ui/toast";
import type { Deadline } from "@/features/courses/api";
import { usePatchEntry, type PlanEntry } from "@/features/plan/api";
import type { Draft } from "@/features/plan/components/entry-dialog";
import {
  atMinute,
  clamp,
  columns,
  dayKey,
  duration,
  hhmm,
  minuteOfDay,
  snap,
  SNAP_MIN,
} from "@/features/plan/lib/week";
import { deadlineName } from "@/features/deadlines/lib/grades";
import { cn } from "@/lib/cn";
import { messageFor } from "@/lib/errors";
import { formatInZone } from "@/lib/time";

const HOUR_PX = 48;
const PX_PER_MIN = HOUR_PX / 60;
const MIN_LENGTH = 30;

type Drag = {
  entry: PlanEntry;
  mode: "move" | "resize";
  x0: number;
  y0: number;
  colWidth: number;
  day0: number;
  start0: number;
  end0: number;
  day: number;
  start: number;
  end: number;
  moved: boolean;
};

/** Where an entry sits: its day (0–6), start and end in minutes after local midnight. */
function place(e: PlanEntry, days: TZDate[], tz: string) {
  const day = days.findIndex((d) => dayKey(d, tz) === dayKey(e.startAt, tz));
  const start = minuteOfDay(e.startAt, tz);
  const end = Math.max(
    start + SNAP_MIN,
    dayKey(e.endAt, tz) === dayKey(e.startAt, tz) ? minuteOfDay(e.endAt, tz) : 24 * 60,
  );
  return { day, start, end };
}

export function WeekCalendar({
  days,
  entries,
  exams,
  span,
  colors,
  onOpen,
  onAdd,
}: {
  days: TZDate[];
  entries: PlanEntry[];
  exams: Deadline[];
  /** The study window, minutes after midnight. */
  span: { start: number; end: number };
  colors: Record<string, string>;
  onOpen: (e: PlanEntry) => void;
  onAdd: (d: Draft) => void;
}) {
  const user = useUser();
  const tz = user.timeZone;
  const patch = usePatchEntry();
  const toast = useToast();
  const [drag, setDrag] = useState<Drag | null>(null);
  // After a drop, hold the new spot until the refetch lands.
  const [held, setHeld] = useState<Record<string, { day: number; start: number; end: number }>>({});
  const grid = useRef<HTMLDivElement>(null);

  const shown = entries.filter((e) => e.status !== "Moved");
  const placed = shown.map((e) => ({ e, ...(held[e.id] ?? place(e, days, tz)) })).filter((p) => p.day >= 0);
  // Widen the window to whatever the plan holds outside it.
  const top = Math.floor(Math.min(span.start, ...placed.map((p) => p.start)) / 60) * 60;
  const bottom = Math.ceil(Math.max(span.end, ...placed.map((p) => p.end)) / 60) * 60;
  const hours = Array.from({ length: (bottom - top) / 60 }, (_, i) => top + i * 60);
  const today = dayKey(new Date(), tz);

  // Window listeners, not pointer capture: the drop must land even when the pointer leaves the block.
  const dragRef = useRef<Drag | null>(null);
  const set = (d: Drag | null) => {
    dragRef.current = d;
    setDrag(d);
  };

  const begin = (ev: ReactPointerEvent, p: (typeof placed)[number], mode: Drag["mode"]) => {
    if (ev.button !== 0 || p.e.status !== "Planned") return;
    ev.stopPropagation();
    ev.preventDefault();
    const colWidth = (grid.current?.clientWidth ?? 700) / 7;
    set({
      entry: p.e,
      mode,
      x0: ev.clientX,
      y0: ev.clientY,
      colWidth,
      day0: p.day,
      start0: p.start,
      end0: p.end,
      day: p.day,
      start: p.start,
      end: p.end,
      moved: false,
    });
    const onMove = (e: PointerEvent) => move(e);
    const onUp = (e: PointerEvent) => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
      if (e.type === "pointercancel") set(null);
      else finish();
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
  };

  const move = (ev: PointerEvent) => {
    const drag = dragRef.current;
    if (!drag) return;
    const dy = snap((ev.clientY - drag.y0) / PX_PER_MIN);
    const moved = drag.moved || Math.abs(ev.clientY - drag.y0) > 4 || Math.abs(ev.clientX - drag.x0) > 4;
    if (drag.mode === "resize") {
      set({ ...drag, moved, end: clamp(drag.end0 + dy, drag.start0 + MIN_LENGTH, 24 * 60) });
      return;
    }
    const length = drag.end0 - drag.start0;
    const start = clamp(drag.start0 + dy, 0, 24 * 60 - length);
    const day = clamp(drag.day0 + Math.round((ev.clientX - drag.x0) / drag.colWidth), 0, 6);
    set({ ...drag, moved, day, start, end: start + length });
  };

  const finish = () => {
    const d = dragRef.current;
    set(null);
    if (!d) return;
    if (!d.moved) return onOpen(d.entry);
    if (d.day === d.day0 && d.start === d.start0 && d.end === d.end0) return;
    const day = days[d.day]!;
    const id = d.entry.id;
    setHeld((h) => ({ ...h, [id]: { day: d.day, start: d.start, end: d.end } }));
    patch.mutate(
      {
        id,
        version: d.entry.version,
        patch: { startAt: atMinute(day, d.start).toISOString(), endAt: atMinute(day, d.end).toISOString() },
      },
      {
        onError: (e) => toast({ message: messageFor(e), tone: "danger" }),
        onSettled: () =>
          setHeld((h) => {
            const { [id]: _, ...rest } = h;
            return rest;
          }),
      },
    );
  };

  // Drag down an empty part of a day to draw a new entry; releasing opens the add dialog with
  // that span. A plain click does nothing (double-click still adds an hour).
  const [sketch, setSketch] = useState<{ day: number; a: number; b: number } | null>(null);
  const sketchAt = (ev: ReactPointerEvent<HTMLDivElement>, i: number) => {
    if (ev.button !== 0 || ev.target !== ev.currentTarget) return;
    const box = ev.currentTarget.getBoundingClientRect();
    const minute = (clientY: number) => clamp(snap(top + (clientY - box.top) / PX_PER_MIN), 0, 24 * 60);
    const a = minute(ev.clientY);
    let last = { day: i, a, b: a };
    setSketch(last);
    const onMove = (e: PointerEvent) => {
      last = { day: i, a, b: minute(e.clientY) };
      setSketch(last);
    };
    const onUp = (e: PointerEvent) => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
      setSketch(null);
      const from = Math.min(last.a, last.b);
      const to = Math.max(last.a, last.b);
      if (e.type === "pointercancel" || to - from < SNAP_MIN) return;
      onAdd({ startAt: atMinute(days[i]!, from), endAt: atMinute(days[i]!, to) });
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
  };

  const addAt = (ev: React.MouseEvent<HTMLDivElement>, i: number) => {
    if (ev.target !== ev.currentTarget) return;
    const y = ev.clientY - ev.currentTarget.getBoundingClientRect().top;
    const start = clamp(Math.floor((top + y / PX_PER_MIN) / 30) * 30, 0, 24 * 60 - 60);
    onAdd({ startAt: atMinute(days[i]!, start), endAt: atMinute(days[i]!, start + 60) });
  };

  return (
    <div className="overflow-hidden rounded-lg border border-border bg-surface">
      <div className="grid grid-cols-[52px_repeat(7,minmax(0,1fr))] border-b border-border">
        <div />
        {days.map((d, i) => {
          const dayExams = exams.filter((x) => dayKey(x.startsAt, tz) === dayKey(d, tz));
          return (
            <div
              key={i}
              className={cn(
                "flex min-w-0 flex-col gap-1 border-l border-border px-2 py-2",
                dayKey(d, tz) === today && "bg-sunken",
              )}
            >
              <span className="text-xs uppercase tracking-[0.06em] text-muted">{formatInZone(d, tz, "EEE")}</span>
              <span className={cn("font-mono text-md tnum", dayKey(d, tz) === today && "font-semibold")}>
                {formatInZone(d, tz, "d")}
              </span>
              {dayExams.map((x) => (
                <span
                  key={x.id}
                  className="flex items-center gap-1 truncate rounded-sm bg-danger-bg px-1.5 py-0.5 text-xs font-medium text-danger-text"
                  title={`${deadlineName(x)} ${formatInZone(x.startsAt, tz, "HH:mm")}`}
                >
                  <Flag className="size-3 shrink-0" aria-hidden="true" />
                  <span className="truncate">
                    {deadlineName(x)} {formatInZone(x.startsAt, tz, "HH:mm")}
                  </span>
                </span>
              ))}
            </div>
          );
        })}
      </div>
      <div className="relative grid grid-cols-[52px_minmax(0,1fr)]">
        <div className="relative" style={{ height: (bottom - top) * PX_PER_MIN }}>
          {hours.map((h) => (
            <span
              key={h}
              className="absolute right-2 -translate-y-1/2 font-mono text-xs text-muted tnum"
              style={{ top: (h - top) * PX_PER_MIN }}
            >
              {h > top ? hhmm(h) : ""}
            </span>
          ))}
        </div>
        <div ref={grid} className="relative grid grid-cols-7" style={{ height: (bottom - top) * PX_PER_MIN }}>
          {hours.map((h) => (
            <div
              key={h}
              aria-hidden="true"
              className="pointer-events-none absolute inset-x-0 border-t border-border"
              style={{ top: (h - top) * PX_PER_MIN }}
            />
          ))}
          {/* Outside the study window: shaded. */}
          <div
            aria-hidden="true"
            className="pointer-events-none absolute inset-x-0 top-0 bg-sunken/60"
            style={{ height: (span.start - top) * PX_PER_MIN }}
          />
          <div
            aria-hidden="true"
            className="pointer-events-none absolute inset-x-0 bottom-0 bg-sunken/60"
            style={{ height: (bottom - span.end) * PX_PER_MIN }}
          />
          {days.map((_, i) => {
            const inDay = placed.filter((p) => (drag?.entry.id === p.e.id ? drag.day : p.day) === i);
            const cols = columns(
              inDay.map((p) => ({
                id: p.e.id,
                start: drag?.entry.id === p.e.id ? drag.start : p.start,
                end: drag?.entry.id === p.e.id ? drag.end : p.end,
              })),
            );
            return (
              <div
                key={i}
                className="relative border-l border-border"
                onDoubleClick={(ev) => addAt(ev, i)}
                onPointerDown={(ev) => sketchAt(ev, i)}
                title="Drag to add, or double-click"
              >
                {sketch?.day === i && sketch.a !== sketch.b ? (
                  <div
                    aria-hidden="true"
                    className="pointer-events-none absolute inset-x-1 z-10 rounded-md border-2 border-dashed border-text/40 bg-sunken/60 px-1.5 text-xs text-muted tnum"
                    style={{
                      top: (Math.min(sketch.a, sketch.b) - top) * PX_PER_MIN,
                      height: Math.abs(sketch.b - sketch.a) * PX_PER_MIN,
                    }}
                  >
                    {hhmm(Math.min(sketch.a, sketch.b))}–{hhmm(Math.max(sketch.a, sketch.b))}
                  </div>
                ) : null}
                {inDay.map((p) => {
                  const live = drag?.entry.id === p.e.id ? drag : null;
                  const start = live?.start ?? p.start;
                  const finish = live?.end ?? p.end;
                  const c = cols.get(p.e.id) ?? { col: 0, of: 1 };
                  const draggable = p.e.status === "Planned";
                  return (
                    <div
                      key={p.e.id}
                      className={cn("absolute px-0.5", live && "z-10 opacity-90")}
                      style={{
                        top: (start - top) * PX_PER_MIN,
                        height: Math.max(20, (finish - start) * PX_PER_MIN),
                        left: `${(c.col / c.of) * 100}%`,
                        width: `${100 / c.of}%`,
                      }}
                    >
                      <button
                        type="button"
                        aria-label={`${p.e.courseCode} ${p.e.description}, ${formatInZone(p.e.startAt, tz, "EEE HH:mm")}, ${duration(p.end - p.start)}, ${p.e.status}`}
                        className={cn(
                          "relative h-full w-full overflow-hidden text-left",
                          draggable ? "cursor-grab touch-none active:cursor-grabbing" : "cursor-pointer",
                        )}
                        onPointerDown={(ev) => begin(ev, p, "move")}
                        onClick={(ev) => {
                          // Pointer clicks open from pointerup; this is the keyboard path.
                          if (ev.detail === 0 || !draggable) onOpen(p.e);
                        }}
                      >
                        <PlanBlock
                          className="h-full overflow-hidden px-1.5 py-1 text-sm"
                          time={`${hhmm(start)}–${hhmm(finish)}`}
                          title={
                            <span className="line-clamp-2">
                              <EntryLabel entry={p.e} />
                            </span>
                          }
                          pinned={p.e.pinned || p.e.createdBy === "User"}
                          color={colors[p.e.courseId]}
                          status={p.e.status as "Planned" | "Done" | "Skipped"}
                        />
                        {draggable ? (
                          <span
                            aria-hidden="true"
                            className="absolute inset-x-0 bottom-0 h-2 cursor-ns-resize"
                            onPointerDown={(ev) => begin(ev, p, "resize")}
                          />
                        ) : null}
                      </button>
                    </div>
                  );
                })}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

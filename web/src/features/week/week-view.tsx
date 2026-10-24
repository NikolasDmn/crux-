import { Link } from "@tanstack/react-router";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { useState } from "react";
import { Bar, BarChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { useUser } from "@/app/auth";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, SectionLabel } from "@/components/ui/card";
import { EmptyList } from "@/components/ui/empty";
import { Dot, Skeleton } from "@/components/ui/misc";
import { useCourses } from "@/features/courses/api";
import { useWeek } from "@/features/next/api";
import { isoWeekOf, parseIsoWeek, shiftWeek } from "@/features/plan/lib/week";
import { barRows, heat, hours, readSummary, type WeekSummary } from "@/features/week/lib/summary";
import { cn } from "@/lib/cn";
import { messageFor } from "@/lib/errors";
import { formatInZone, relativeDays } from "@/lib/time";

// This week (frontend-spec 4): planned vs actual hours per day per course, slips, totals,
// on-track per exam, arrows between weeks, a daily heatmap. Every chart has "Show as table" (8).
export function WeekView({ isoWeek, onWeek }: { isoWeek?: string | undefined; onWeek: (w: string) => void }) {
  const user = useUser();
  const tz = user.timeZone;
  const thisWeek = isoWeekOf(new Date(), tz);
  const week = isoWeek && parseIsoWeek(isoWeek, tz) ? isoWeek : thisWeek;
  const data = useWeek(week);
  const courses = useCourses();
  const colors = Object.fromEntries((courses.data ?? []).map((c) => [c.id, c.color]));
  const s = data.data ? readSummary(data.data.summary) : null;
  const start = parseIsoWeek(week, tz)!;

  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-wrap items-center gap-2">
        <h1 className="m-0 mr-2 text-xl font-semibold tracking-[-0.01em]">This week</h1>
        <Button
          size="iconSm"
          variant="ghost"
          aria-label="Previous week"
          onClick={() => onWeek(shiftWeek(week, -1, tz))}
        >
          <ChevronLeft className="size-4" />
        </Button>
        <span className="font-mono text-sm tnum">
          {formatInZone(start, tz, "d MMM")} – {formatInZone(new Date(start.getTime() + 6 * 86_400_000), tz, "d MMM")}
        </span>
        <Button
          size="iconSm"
          variant="ghost"
          aria-label="Next week"
          disabled={week >= thisWeek}
          onClick={() => onWeek(shiftWeek(week, 1, tz))}
        >
          <ChevronRight className="size-4" />
        </Button>
        {week !== thisWeek ? (
          <Button size="sm" variant="ghost" onClick={() => onWeek(thisWeek)}>
            This week
          </Button>
        ) : null}
        <Button asChild size="sm" className="ml-auto">
          <Link to="/plan" search={{ w: week }}>
            Open the plan
          </Link>
        </Button>
      </div>
      {data.isPending ? (
        <Skeleton className="h-80" />
      ) : data.isError ? (
        <EmptyList title="Couldn't load this week" body={messageFor(data.error)} />
      ) : s && (s.courses.length || s.days.some((d) => d.courses.length)) ? (
        <>
          <Totals s={s} />
          <HoursChart s={s} colors={colors} tz={tz} />
          <CoursesTable s={s} colors={colors} />
          <Heatmap s={s} colors={colors} tz={tz} />
        </>
      ) : (
        <EmptyList
          title="Nothing tracked this week"
          body="Study time shows up here as you open materials, answer questions and review cards."
        />
      )}
    </div>
  );
}

function Stat({ label, value, detail }: { label: string; value: string; detail?: string }) {
  return (
    <Card className="flex flex-col gap-1 py-4">
      <SectionLabel>{label}</SectionLabel>
      <span className="font-mono text-2xl font-semibold tnum">{value}</span>
      {detail ? <span className="text-sm text-muted">{detail}</span> : null}
    </Card>
  );
}

function Totals({ s }: { s: WeekSummary }) {
  const t = s.totals;
  return (
    <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
      <Stat label="Studied" value={hours(t.actualHours)} detail={`of ${hours(t.plannedHours)} planned`} />
      <Stat label="Steps done" value={String(t.stepsDone)} />
      <Stat label="MC first try" value={t.mcFirstTryRate === null ? "–" : `${Math.round(t.mcFirstTryRate * 100)}%`} />
      <Stat label="Slips" value={String(t.slips)} />
    </div>
  );
}

function TableToggle({ table, onChange }: { table: boolean; onChange: (t: boolean) => void }) {
  return (
    <Button size="sm" variant="ghost" aria-pressed={table} onClick={() => onChange(!table)}>
      {table ? "Show as chart" : "Show as table"}
    </Button>
  );
}

function HoursChart({ s, colors, tz }: { s: WeekSummary; colors: Record<string, string>; tz: string }) {
  const [table, setTable] = useState(false);
  const label = (date: string) => formatInZone(`${date}T12:00:00Z`, tz, "EEE");
  const rows = barRows(s, label);
  const ids = s.courses.map((c) => c.courseId);
  const code = Object.fromEntries(s.courses.map((c) => [c.courseId, c.code]));
  return (
    <Card className="flex flex-col gap-3">
      <div className="flex items-center justify-between gap-2">
        <SectionLabel>Hours per day</SectionLabel>
        <TableToggle table={table} onChange={setTable} />
      </div>
      {table ? (
        <table className="w-full border-collapse text-sm">
          <thead>
            <tr className="text-left text-xs uppercase tracking-[0.06em] text-muted">
              <th className="py-1.5 font-medium">Day</th>
              <th className="py-1.5 font-medium">Planned</th>
              {ids.map((id) => (
                <th key={id} className="py-1.5 font-medium">
                  {code[id]}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={String(r.label)} className="h-8 border-t border-border">
                <td>{r.label}</td>
                <td className="font-mono tnum">{hours(Number(r.planned))}</td>
                {ids.map((id) => (
                  <td key={id} className="font-mono tnum">
                    {hours(Number(r[id] ?? 0))}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      ) : (
        <div className="h-64" role="img" aria-label="Hours studied per day by course, against planned hours">
          <ResponsiveContainer width="100%" height="100%">
            <BarChart data={rows} barGap={2} margin={{ top: 4, right: 4, bottom: 0, left: -16 }}>
              <CartesianGrid vertical={false} stroke="var(--border)" />
              <XAxis
                dataKey="label"
                tickLine={false}
                axisLine={false}
                tick={{ fill: "var(--text-muted)", fontSize: 12 }}
              />
              <YAxis
                tickLine={false}
                axisLine={false}
                tick={{ fill: "var(--text-muted)", fontSize: 12 }}
                allowDecimals={false}
              />
              <Tooltip
                cursor={{ fill: "var(--surface-sunken)" }}
                contentStyle={{
                  background: "var(--surface-raised)",
                  border: "1px solid var(--border)",
                  borderRadius: 6,
                  color: "var(--text)",
                  fontSize: 13,
                }}
                formatter={(v, name) => [
                  hours(Number(v)),
                  name === "planned" ? "Planned" : (code[String(name)] ?? name),
                ]}
              />
              <Bar
                dataKey="planned"
                fill="transparent"
                stroke="var(--text-muted)"
                strokeDasharray="3 3"
                radius={[3, 3, 0, 0]}
              />
              {ids.map((id, n) => (
                <Bar
                  key={id}
                  dataKey={id}
                  stackId="actual"
                  fill={colors[id] ?? "var(--primary)"}
                  radius={n === ids.length - 1 ? [3, 3, 0, 0] : 0}
                />
              ))}
            </BarChart>
          </ResponsiveContainer>
        </div>
      )}
      <div className="flex flex-wrap gap-x-4 gap-y-1 text-sm text-muted">
        <span className="flex items-center gap-1.5">
          <span className="inline-block size-3 rounded-sm border border-dashed border-muted" aria-hidden="true" />
          Planned
        </span>
        {s.courses.map((c) => (
          <span key={c.courseId} className="flex items-center gap-1.5">
            <Dot color={colors[c.courseId]} />
            {c.code}
          </span>
        ))}
      </div>
    </Card>
  );
}

function CoursesTable({ s, colors }: { s: WeekSummary; colors: Record<string, string> }) {
  const user = useUser();
  return (
    <Card className="overflow-x-auto p-0">
      <table className="w-full min-w-[640px] border-collapse text-sm">
        <thead>
          <tr className="text-left text-xs uppercase tracking-[0.06em] text-muted">
            <th className="px-5 py-2.5 font-medium">Course</th>
            <th className="py-2.5 font-medium">Hours</th>
            <th className="py-2.5 font-medium">MC first try</th>
            <th className="py-2.5 font-medium">Slips</th>
            <th className="py-2.5 font-medium">Weak spots</th>
            <th className="py-2.5 pr-5 font-medium">Exam</th>
          </tr>
        </thead>
        <tbody>
          {s.courses.map((c) => (
            <tr key={c.courseId} className="h-10 border-t border-border">
              <td className="px-5">
                <Link
                  to="/c/$courseId"
                  params={{ courseId: c.courseId }}
                  className="flex items-center gap-2 font-medium hover:underline"
                >
                  <Dot color={colors[c.courseId]} />
                  {c.code}
                </Link>
              </td>
              <td className="font-mono tnum">
                {hours(c.actualHours)} <span className="text-muted">/ {hours(c.plannedHours)}</span>
              </td>
              <td className="font-mono tnum">
                {c.mcTries ? `${Math.round((c.mcRights / c.mcTries) * 100)}% (${c.mcTries})` : "–"}
              </td>
              <td className="font-mono tnum">{c.slips}</td>
              <td className="font-mono tnum">
                +{c.weakSpotsOpened} / −{c.weakSpotsResolved}
              </td>
              <td className="pr-5">
                {c.nextExamAt ? (
                  <span className="flex items-center gap-2">
                    {relativeDays(c.nextExamAt, user.timeZone)}
                    {c.onTrack === true ? (
                      <Badge kind="status" tone="success">
                        On track
                      </Badge>
                    ) : c.onTrack === false ? (
                      <Badge kind="status" tone="warning">
                        Behind
                      </Badge>
                    ) : null}
                  </span>
                ) : (
                  <span className="text-muted">–</span>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </Card>
  );
}

const HEAT = ["bg-sunken", "opacity-30", "opacity-50", "opacity-75", "opacity-100"] as const;

function Heatmap({ s, colors, tz }: { s: WeekSummary; colors: Record<string, string>; tz: string }) {
  const [table, setTable] = useState(false);
  const max = Math.max(0, ...s.days.flatMap((d) => d.courses.map((c) => c.actualHours)));
  const cell = (date: string, courseId: string) =>
    s.days.find((d) => d.date === date)?.courses.find((c) => c.courseId === courseId);
  return (
    <Card className="flex flex-col gap-3">
      <div className="flex items-center justify-between gap-2">
        <SectionLabel>Day by day</SectionLabel>
        <TableToggle table={table} onChange={setTable} />
      </div>
      <div className="overflow-x-auto">
        <table className="w-full min-w-[480px] border-separate border-spacing-1 text-sm">
          <thead>
            <tr>
              <th />
              {s.days.map((d) => (
                <th key={d.date} className="text-xs font-medium uppercase tracking-[0.06em] text-muted">
                  {formatInZone(`${d.date}T12:00:00Z`, tz, "EEE")}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {s.courses.map((c) => (
              <tr key={c.courseId}>
                <th className="pr-2 text-left font-mono text-xs font-medium">{c.code}</th>
                {s.days.map((d) => {
                  const x = cell(d.date, c.courseId);
                  const h = x?.actualHours ?? 0;
                  const level = heat(h, max);
                  const text = `${hours(h)}${x?.slips ? ` · ${x.slips} slip${x.slips === 1 ? "" : "s"}` : ""}`;
                  return (
                    <td key={d.date} className="h-9 p-0" title={`${c.code} ${d.date}: ${text}`}>
                      {table ? (
                        <span className="font-mono text-xs tnum">{text}</span>
                      ) : (
                        <span className="relative block h-9 rounded-sm bg-sunken">
                          {level ? (
                            <span
                              className={cn("absolute inset-0 rounded-sm", HEAT[level])}
                              style={{ background: colors[c.courseId] ?? "var(--primary)" }}
                            />
                          ) : null}
                          <span className="sr-only">{text}</span>
                        </span>
                      )}
                    </td>
                  );
                })}
              </tr>
            ))}
            <tr>
              <th className="pr-2 text-left text-xs font-medium text-muted">Steps</th>
              {s.days.map((d) => (
                <td key={d.date} className="text-center font-mono text-xs text-muted tnum">
                  {d.stepsDone}
                </td>
              ))}
            </tr>
          </tbody>
        </table>
      </div>
    </Card>
  );
}

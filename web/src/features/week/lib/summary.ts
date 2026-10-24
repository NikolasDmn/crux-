// The week summary (`GET /week`'s `summary`, untyped in the API): read defensively into a shape the
// screen can trust, since the nightly job writes it and older rows may miss fields.

export interface DayCourse {
  courseId: string;
  plannedHours: number;
  actualHours: number;
  slips: number;
}

export interface Day {
  date: string;
  stepsDone: number;
  courses: DayCourse[];
}

export interface WeekCourse {
  courseId: string;
  code: string;
  plannedHours: number;
  actualHours: number;
  mcTries: number;
  mcRights: number;
  slips: number;
  weakSpotsOpened: number;
  weakSpotsResolved: number;
  nextExamAt: string | null;
  onTrack: boolean | null;
}

export interface WeekSummary {
  from: string;
  to: string;
  days: Day[];
  courses: WeekCourse[];
  totals: {
    plannedHours: number;
    actualHours: number;
    stepsDone: number;
    slips: number;
    mcFirstTryRate: number | null;
  };
}

type Obj = Record<string, unknown>;
const obj = (v: unknown): Obj => (v && typeof v === "object" && !Array.isArray(v) ? (v as Obj) : {});
const arr = (v: unknown): unknown[] => (Array.isArray(v) ? v : []);
const num = (v: unknown): number => (typeof v === "number" && Number.isFinite(v) ? v : 0);
const str = (v: unknown): string => (typeof v === "string" ? v : "");

export function readSummary(raw: unknown): WeekSummary {
  const s = obj(raw);
  const t = obj(s.totals);
  return {
    from: str(s.from),
    to: str(s.to),
    days: arr(s.days).map((d) => {
      const o = obj(d);
      return {
        date: str(o.date),
        stepsDone: num(o.stepsDone),
        courses: arr(o.courses).map((c) => {
          const x = obj(c);
          return {
            courseId: str(x.courseId),
            plannedHours: num(x.plannedHours),
            actualHours: num(x.actualHours),
            slips: num(x.slips),
          };
        }),
      };
    }),
    courses: arr(s.courses).map((c) => {
      const x = obj(c);
      const mc = obj(x.mcFirstTry);
      return {
        courseId: str(x.courseId),
        code: str(x.code),
        plannedHours: num(x.plannedHours),
        actualHours: num(x.actualHours),
        mcTries: num(mc.tries),
        mcRights: num(mc.rights),
        slips: num(x.slips),
        weakSpotsOpened: num(x.weakSpotsOpened),
        weakSpotsResolved: num(x.weakSpotsResolved),
        nextExamAt: typeof x.nextExamAt === "string" ? x.nextExamAt : null,
        onTrack: typeof x.onTrack === "boolean" ? x.onTrack : null,
      };
    }),
    totals: {
      plannedHours: num(t.plannedHours),
      actualHours: num(t.actualHours),
      stepsDone: num(t.stepsDone),
      slips: num(t.slips),
      mcFirstTryRate: typeof t.mcFirstTryRate === "number" ? t.mcFirstTryRate : null,
    },
  };
}

/** One row per day for the stacked bars: `{label, planned, [courseId]: actual}`. */
export function barRows(s: WeekSummary, label: (date: string) => string): Record<string, number | string>[] {
  return s.days.map((d) => {
    const row: Record<string, number | string> = { label: label(d.date), planned: 0 };
    for (const c of d.courses) {
      row.planned = (row.planned as number) + c.plannedHours;
      row[c.courseId] = ((row[c.courseId] as number | undefined) ?? 0) + c.actualHours;
    }
    return row;
  });
}

/** 0–4 intensity for the heatmap: actual hours against the busiest cell of the week. */
export function heat(hours: number, max: number): 0 | 1 | 2 | 3 | 4 {
  if (hours <= 0 || max <= 0) return 0;
  return Math.min(4, Math.max(1, Math.ceil((hours / max) * 4))) as 1 | 2 | 3 | 4;
}

/** "1.5 h", "0 h", "45 min" under an hour. */
export function hours(h: number): string {
  if (h > 0 && h < 1) return `${Math.round(h * 60)} min`;
  return `${Number.isInteger(h) ? h : h.toFixed(1)} h`;
}

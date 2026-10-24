import { nextExam, type Deadline } from "../api";

const d = (id: string, type: string, startsAt: string, status = "Upcoming") =>
  ({ id, type, startsAt, status }) as Deadline;

it("picks the nearest upcoming exam or resit", () => {
  const now = new Date("2026-10-24T12:00:00Z");
  const list = [
    d("a", "Deadline", "2026-10-25T12:00:00Z"),
    d("b", "Exam", "2026-11-20T08:00:00Z"),
    d("c", "Resit", "2026-11-03T08:00:00Z"),
    d("d", "Exam", "2026-10-20T08:00:00Z"),
    d("e", "Exam", "2026-10-30T08:00:00Z", "Cancelled"),
  ];
  expect(nextExam(list, now)?.id).toBe("c");
  expect(nextExam([], now)).toBeUndefined();
});

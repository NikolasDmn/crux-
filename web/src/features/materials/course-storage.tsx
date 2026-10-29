import { useQueries } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { useState } from "react";
import { useUser } from "@/app/auth";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, SectionLabel } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/misc";
import { Dialog } from "@/components/ui/overlays";
import { useToast } from "@/components/ui/toast";
import type { Course } from "@/features/courses/api";
import { isOwner } from "@/features/courses/lib/roles";
import { getFile, useCancelFree, useFreeCourse, useFreeMaterial, useStorage } from "@/features/files/api";
import { formatBytes } from "@/features/files/lib/upload";
import { keys, useLectures, type Material } from "@/features/lectures/api";
import { api, unwrap } from "@/lib/api";
import { messageFor } from "@/lib/errors";
import { formatInZone } from "@/lib/time";

// A course's Storage section (frontend-spec 4, course settings): per-material size and "Free up
// space", the course-wide button (past papers too), pending removals with Undo for 7 days. Only the
// owner, whose quota the files use (D2-4), sees the buttons.

function MaterialRow({
  courseId,
  m,
  lecture,
  size,
}: {
  courseId: string;
  m: Material;
  lecture: string;
  size: number | undefined;
}) {
  const user = useUser();
  const free = useFreeMaterial();
  const cancel = useCancelFree();
  const toast = useToast();
  const onError = (e: unknown) => toast({ message: messageFor(e), tone: "danger" });
  return (
    <li className="flex flex-wrap items-center gap-x-3 gap-y-1 py-2.5">
      <span className="flex min-w-0 flex-1 flex-col">
        <Link
          to="/c/$courseId/m/$materialId"
          params={{ courseId, materialId: m.id }}
          className="truncate underline-offset-4 hover:underline"
        >
          {m.title}
        </Link>
        <span className="text-sm text-muted">{lecture}</span>
      </span>
      <span className="w-20 text-right font-mono text-sm tnum text-muted">
        {size === undefined ? "…" : formatBytes(size)}
      </span>
      <span className="flex w-56 items-center justify-end gap-2">
        {m.originalStatus === "Purged" ? (
          <Badge tone="info">Original removed</Badge>
        ) : m.originalStatus === "PendingPurge" ? (
          <>
            <span className="text-sm text-muted">
              Goes {m.originalPurgeAt ? formatInZone(m.originalPurgeAt, user.timeZone, "d MMM") : "soon"}
            </span>
            <Button size="sm" disabled={cancel.isPending} onClick={() => cancel.mutate(m.id, { onError })}>
              Undo
            </Button>
          </>
        ) : (
          <Button size="sm" variant="ghost" disabled={free.isPending} onClick={() => free.mutate(m.id, { onError })}>
            Free up space
          </Button>
        )}
      </span>
    </li>
  );
}

function OwnerStorage({ course }: { course: Course }) {
  const user = useUser();
  const toast = useToast();
  const lectures = useLectures(course.id);
  const lists = useQueries({
    queries: (lectures.data ?? []).map((l) => ({
      queryKey: keys.materials(l.id),
      queryFn: async () =>
        (await unwrap(api.GET("/components/{id}/materials", { params: { path: { id: l.id } } }))).sort(
          (a, b) => a.sortOrder - b.sortOrder,
        ),
    })),
  });
  const rows = (lectures.data ?? []).flatMap((l, i) =>
    (lists[i]?.data ?? []).filter((m) => m.fileId).map((m) => ({ m, lecture: `${l.code} ${l.name}` })),
  );
  const fileIds = [...new Set(rows.map((r) => r.m.fileId!))];
  const files = useQueries({
    queries: fileIds.map((id) => ({ queryKey: ["file", id], queryFn: () => getFile(id), staleTime: 60_000 })),
  });
  const sizes = new Map(files.flatMap((f) => (f.data ? [[f.data.id, f.data.sizeBytes] as const] : [])));
  const storage = useStorage();
  const usage = storage.data?.byCourse.find((u) => u.courseId === course.id);
  const freeAll = useFreeCourse(course.id);
  const [confirm, setConfirm] = useState(false);
  const loading = lectures.isPending || lists.some((l) => l.isPending);
  const present = rows.filter((r) => r.m.originalStatus === "Present").length;

  return (
    <Card className="flex flex-col gap-3">
      <SectionLabel>Storage</SectionLabel>
      <p className="m-0 text-muted">
        {usage ? `This course uses ${formatBytes(usage.bytes)} of your storage. ` : ""}
        Freeing up space removes a PDF's original after 7 days. Its text, question images and thumbnails stay, so
        search, citations and the AI tutor keep working. Re-upload the same PDF to bring it back.
      </p>
      {loading ? (
        <Skeleton className="h-24" />
      ) : rows.length ? (
        <ul className="m-0 flex list-none flex-col divide-y divide-border p-0">
          {rows.map((r) => (
            <MaterialRow key={r.m.id} courseId={course.id} m={r.m} lecture={r.lecture} size={sizes.get(r.m.fileId!)} />
          ))}
        </ul>
      ) : (
        <p className="m-0 text-sm text-muted">No uploaded PDFs in this course.</p>
      )}
      <div className="flex flex-wrap items-center gap-3">
        <Button variant="secondary" onClick={() => setConfirm(true)} disabled={loading}>
          Free up space for the whole course
        </Button>
        {present && usage?.purgeable ? (
          <span className="text-sm text-muted">Up to {formatBytes(usage.purgeable)} now</span>
        ) : null}
      </div>
      <Dialog
        open={confirm}
        onOpenChange={setConfirm}
        title={`Free up space in ${course.code}?`}
        description={`The originals of ${present} ${present === 1 ? "material" : "materials"} and of the course's past papers are removed in 7 days. Until then you can undo each material here.`}
        footer={
          <>
            <Button onClick={() => setConfirm(false)}>Cancel</Button>
            <Button
              variant="primary"
              disabled={freeAll.isPending}
              onClick={() =>
                freeAll.mutate(undefined, {
                  onSuccess: (r) => {
                    setConfirm(false);
                    toast({
                      message: r.files
                        ? `${r.files} ${r.files === 1 ? "original goes" : "originals go"} on ${formatInZone(r.purgeAt, user.timeZone, "d MMM")}`
                        : "Nothing left to free up",
                    });
                  },
                  onError: (e) => toast({ message: messageFor(e), tone: "danger" }),
                })
              }
            >
              Free up space
            </Button>
          </>
        }
      />
    </Card>
  );
}

export function CourseStorage({ course }: { course: Course }) {
  if (isOwner(course.myRole)) return <OwnerStorage course={course} />;
  return (
    <Card className="flex flex-col gap-2">
      <SectionLabel>Storage</SectionLabel>
      <p className="m-0 text-muted">
        This course's files count against its owner's storage. Only they can free up space.
      </p>
    </Card>
  );
}

import { createFileRoute, Link } from "@tanstack/react-router";
import { Banner } from "@/components/ui/banner";
import { Card, SectionLabel } from "@/components/ui/card";
import { Check } from "@/components/ui/field";
import { Skeleton } from "@/components/ui/misc";
import { ProgressBar } from "@/components/ui/progress";
import { useToast } from "@/components/ui/toast";
import { useCourses } from "@/features/courses/api";
import { useStorage, useStorageSettings, type StorageView } from "@/features/files/api";
import { usageRows, usedShare } from "@/features/files/lib/storage";
import { formatBytes } from "@/features/files/lib/upload";
import { messageFor } from "@/lib/errors";

// Storage (frontend-spec 4): used vs quota with the tier, a per-course breakdown with what the saver
// could free, and the two saver settings. Freeing space is the course owner's, in course settings.
export const Route = createFileRoute("/_app/settings/storage")({ component: Storage });

function Usage({ view }: { view: StorageView }) {
  const share = usedShare(view.usedBytes + view.reservedBytes, view.quotaBytes);
  return (
    <Card className="flex flex-col gap-3">
      <SectionLabel>Used</SectionLabel>
      <ProgressBar
        tone="text"
        value={view.usedBytes}
        max={view.quotaBytes}
        label={view.tier ? `${view.tier[0]!.toUpperCase()}${view.tier.slice(1)} plan` : "Your plan"}
        detail={`${formatBytes(view.usedBytes)} of ${formatBytes(view.quotaBytes)}`}
      />
      {view.reservedBytes > 0 ? (
        <p className="m-0 text-sm text-muted">
          Plus {formatBytes(view.reservedBytes)} held for uploads that are still going.
        </p>
      ) : null}
      {share >= 0.9 ? (
        <Banner tone={share >= 1 ? "danger" : "warning"}>
          {share >= 1 ? "You're out of space, so new uploads will fail." : "You're nearly out of space."} Free up space
          from originals below, or delete files you no longer need.
        </Banner>
      ) : null}
    </Card>
  );
}

function ByCourse({ view }: { view: StorageView }) {
  const courses = useCourses();
  const rows = usageRows(view.byCourse, courses.data ?? []);
  return (
    <Card className="flex flex-col gap-3">
      <SectionLabel>By course</SectionLabel>
      {rows.length ? (
        <div className="-mx-1 overflow-x-auto px-1">
          <table className="w-full border-collapse text-left">
            <thead>
              <tr className="text-sm text-muted">
                <th className="py-2 pr-4 font-medium">Course</th>
                <th className="py-2 pr-4 text-right font-medium">Used</th>
                <th className="py-2 pr-4 text-right font-medium">Could free</th>
                <th className="py-2 font-medium">
                  <span className="sr-only">Actions</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => (
                <tr key={r.courseId ?? "none"} className="border-t border-border align-top">
                  <td className="py-2.5 pr-4">
                    <span className="flex flex-col">
                      <span>
                        {r.code ? <span className="mr-2 font-mono text-sm text-muted">{r.code}</span> : null}
                        {r.name}
                      </span>
                      <span className="text-sm text-muted">
                        {r.originals} {r.originals === 1 ? "PDF" : "PDFs"}
                        {r.pendingPurge > 0 ? ` · ${formatBytes(r.pendingPurge)} being removed` : ""}
                      </span>
                    </span>
                  </td>
                  <td className="py-2.5 pr-4 text-right font-mono tnum">{formatBytes(r.bytes)}</td>
                  <td className="py-2.5 pr-4 text-right font-mono tnum">
                    {r.owned && r.purgeable > 0 ? formatBytes(r.purgeable) : "–"}
                  </td>
                  <td className="py-2.5 text-right">
                    {r.owned && r.courseId && (r.purgeable > 0 || r.pendingPurge > 0) ? (
                      <Link
                        to="/c/$courseId/settings"
                        params={{ courseId: r.courseId }}
                        className="whitespace-nowrap text-sm underline underline-offset-4"
                      >
                        Free up space
                      </Link>
                    ) : null}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <p className="m-0 text-muted">Nothing uploaded yet.</p>
      )}
      <p className="m-0 text-sm text-muted">
        "Could free" is the original PDFs of your own courses whose text is already read. Removing an original keeps its
        text, question images and thumbnails, so search, citations and the AI tutor still work. Files in a course you
        share count against the course owner.
      </p>
    </Card>
  );
}

function Saver({ view }: { view: StorageView }) {
  const save = useStorageSettings();
  const toast = useToast();
  const set = (body: { autoPurgeOriginals?: boolean; keepThumbnails?: boolean }) =>
    save.mutate(body, {
      onSuccess: () => toast({ message: "Saved" }),
      onError: (e) => toast({ message: messageFor(e), tone: "danger" }),
    });
  return (
    <Card className="flex flex-col gap-4">
      <SectionLabel>Storage saver</SectionLabel>
      <Check
        checked={view.autoPurgeOriginals}
        disabled={save.isPending}
        onChange={(v) => set({ autoPurgeOriginals: v })}
        label="Remove originals automatically"
        hint="For new uploads: once a PDF's text is read, its original is removed after 7 days. You can undo it in course settings until then, or re-upload the same PDF later to bring it back."
      />
      <Check
        checked={view.keepThumbnails}
        disabled={save.isPending}
        onChange={(v) => set({ keepThumbnails: v })}
        label="Keep page thumbnails"
        hint="Small pictures of each page, counted in your storage, so a removed original still shows its pages and not only its text. Turning this off deletes them."
      />
    </Card>
  );
}

function Storage() {
  const storage = useStorage();
  if (storage.isError) return <Banner tone="danger">{messageFor(storage.error)}</Banner>;
  if (!storage.data) return <Skeleton className="h-96" />;
  return (
    <div className="flex max-w-[760px] flex-col gap-4">
      <Usage view={storage.data} />
      <ByCourse view={storage.data} />
      <Saver view={storage.data} />
    </div>
  );
}

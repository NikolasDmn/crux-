import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useState } from "react";
import { useUser } from "@/app/auth";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { EmptyList } from "@/components/ui/empty";
import { Skeleton } from "@/components/ui/misc";
import { Segmented } from "@/components/ui/segmented";
import { useToast } from "@/components/ui/toast";
import { useAnswerInvitation, useInvitations } from "@/features/courses/api";
import { downloadFile } from "@/features/files/api";
import { useInbox, useMarkAllRead, useMarkRead, type InboxItem } from "@/features/notify/api";
import { inboxHref } from "@/features/notify/lib/link";
import { cn } from "@/lib/cn";
import { messageFor } from "@/lib/errors";
import { formatInZone, relativeDays } from "@/lib/time";
import { t } from "@/lib/t";
import { useVirtualRows } from "@/components/ui/virtual";

// Inbox (frontend-spec 4): delivered in-app notifications, live over SSE (the shell keeps the
// stream open), cursor paging, mark one / all read, deep links, and the inline actions: course
// invitation Accept / Decline and the export download. The check-in opens the course's Exams page.
export const Route = createFileRoute("/_app/inbox")({ component: Inbox });

type Filter = "all" | "unread";

function when(iso: string, tz: string): string {
  return relativeDays(iso, tz) === "today" ? formatInZone(iso, tz, "HH:mm") : formatInZone(iso, tz, "EEE d MMM");
}

function str(v: unknown): string | null {
  return typeof v === "string" && v ? v : null;
}

function InvitationActions({ courseId }: { courseId: string }) {
  const invitations = useInvitations();
  const answer = useAnswerInvitation();
  const toast = useToast();
  if (invitations.isPending) return null;
  if (!invitations.data?.some((i) => i.courseId === courseId))
    return <span className="text-sm text-muted">{t("inbox.answered")}</span>;
  const reply = (accept: boolean) =>
    answer.mutate({ courseId, accept }, { onError: (e) => toast({ message: messageFor(e), tone: "danger" }) });
  return (
    <div className="flex gap-2">
      <Button size="sm" variant="primary" disabled={answer.isPending} onClick={() => reply(true)}>
        {t("invite.accept")}
      </Button>
      <Button size="sm" disabled={answer.isPending} onClick={() => reply(false)}>
        {t("invite.decline")}
      </Button>
    </div>
  );
}

function ExportAction({ fileId, expiresAt }: { fileId: string; expiresAt: string | null }) {
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  if (expiresAt && new Date(expiresAt).getTime() <= Date.now())
    return <span className="text-sm text-muted">{t("inbox.exportExpired")}</span>;
  return (
    <Button
      size="sm"
      disabled={busy}
      onClick={() => {
        setBusy(true);
        downloadFile(fileId)
          .catch((e: unknown) => toast({ message: messageFor(e), tone: "danger" }))
          .finally(() => setBusy(false));
      }}
    >
      {t("inbox.download")}
    </Button>
  );
}

function Actions({ item }: { item: InboxItem }) {
  const p = item.payload;
  const courseId = str(p.courseId);
  const fileId = str(p.fileId);
  if (item.kind === "course.shared" && courseId) return <InvitationActions courseId={courseId} />;
  if (item.kind === "account.export_ready" && fileId)
    return <ExportAction fileId={fileId} expiresAt={str(p.expiresAt)} />;
  return null;
}

function Row({
  item,
  index,
  measure,
}: {
  item: InboxItem;
  index: number;
  measure?: ((el: Element | null) => void) | undefined;
}) {
  const user = useUser();
  const navigate = useNavigate();
  const markRead = useMarkRead();
  const href = inboxHref(item.link);
  const unread = !item.readAt;
  const read = () => {
    if (unread) markRead.mutate(item.id);
  };
  const open = () => {
    read();
    if (href) void navigate({ href });
  };
  return (
    <li ref={measure} data-index={index} className="flex gap-3 px-5 py-3.5">
      <span
        aria-hidden
        className={cn("mt-[7px] size-2 shrink-0 rounded-full", unread ? "bg-primary" : "bg-transparent")}
      />
      <div className="flex min-w-0 flex-1 flex-col gap-1.5">
        <div className="flex items-baseline gap-3">
          {href ? (
            <button
              type="button"
              onClick={open}
              className={cn("min-w-0 flex-1 text-left hover:underline", unread && "font-semibold")}
            >
              {item.title}
            </button>
          ) : (
            <span className={cn("min-w-0 flex-1", unread && "font-semibold")}>{item.title}</span>
          )}
          <span className="shrink-0 text-sm text-muted tnum">{when(item.createdAt, user.timeZone)}</span>
        </div>
        {item.body ? <p className="m-0 text-sm text-muted">{item.body}</p> : null}
        <div className="flex flex-wrap items-center gap-3 empty:hidden">
          <Actions item={item} />
          {unread ? (
            <Button variant="link" className="h-auto p-0 text-sm" disabled={markRead.isPending} onClick={read}>
              {t("inbox.markRead")}
            </Button>
          ) : null}
        </div>
      </div>
    </li>
  );
}

function Inbox() {
  const [filter, setFilter] = useState<Filter>("all");
  const inbox = useInbox(filter === "unread");
  const markAll = useMarkAllRead();
  const toast = useToast();
  const items = inbox.data?.pages.flatMap((p) => p.items) ?? [];
  const unread = inbox.data?.pages[0]?.unread ?? 0;
  const virt = useVirtualRows<HTMLUListElement>(items.length, 76);

  return (
    <div className="flex max-w-[760px] flex-col gap-4">
      <div className="flex flex-wrap items-center gap-3">
        <h1 className="m-0 flex-1 text-xl font-semibold tracking-[-0.01em]">
          {t("nav.inbox")}
          {unread ? (
            <span className="ml-2 text-base font-normal text-muted tnum">{t("inbox.unread", { n: unread })}</span>
          ) : null}
        </h1>
        <Segmented
          size="sm"
          label={t("inbox.filter")}
          value={filter}
          onChange={setFilter}
          options={[
            { value: "all", label: t("inbox.all") },
            { value: "unread", label: t("inbox.unreadOnly") },
          ]}
        />
        <Button
          size="sm"
          disabled={!unread || markAll.isPending}
          onClick={() =>
            markAll.mutate(undefined, { onError: (e) => toast({ message: messageFor(e), tone: "danger" }) })
          }
        >
          {t("inbox.markAll")}
        </Button>
      </div>
      {inbox.isPending ? (
        <Skeleton className="h-40" />
      ) : inbox.isError ? (
        <EmptyList
          title={t("error.title")}
          body={messageFor(inbox.error)}
          action={
            <Button size="sm" onClick={() => void inbox.refetch()}>
              {t("common.retry")}
            </Button>
          }
        />
      ) : !items.length ? (
        <EmptyList
          title={filter === "unread" ? t("inbox.emptyUnread") : t("inbox.empty")}
          body={t("inbox.emptyBody")}
        />
      ) : (
        <Card className="p-0">
          <ul ref={virt.ref} className="m-0 flex list-none flex-col divide-y divide-border p-0">
            {virt.before ? <li aria-hidden="true" style={{ height: virt.before }} /> : null}
            {virt.indices.map((n) => (
              <Row key={items[n]!.id} item={items[n]!} index={n} measure={virt.measure} />
            ))}
            {virt.after ? <li aria-hidden="true" style={{ height: virt.after }} /> : null}
          </ul>
        </Card>
      )}
      {inbox.hasNextPage ? (
        <Button className="self-start" disabled={inbox.isFetchingNextPage} onClick={() => void inbox.fetchNextPage()}>
          {t("inbox.more")}
        </Button>
      ) : null}
    </div>
  );
}

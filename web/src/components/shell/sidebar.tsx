import { Link, useRouterState } from "@tanstack/react-router";
import {
  ArrowRight,
  CalendarDays,
  CalendarRange,
  CircleAlert,
  Home,
  Inbox,
  Layers,
  Plus,
  Settings,
} from "lucide-react";
import type { ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { Dot, Mark } from "@/components/ui/misc";
import { AccountMenu } from "@/components/shell/account-menu";
import { useAnswerInvitation, useCourses, useInvitations, useNextExams } from "@/features/courses/api";
import { useDueCount } from "@/features/flash/api";
import { useUnreadCount } from "@/features/notify/api";
import { cn } from "@/lib/cn";
import { shortUntil } from "@/lib/time";
import { t } from "@/lib/t";

// The 248 px sidebar (frontend-spec 3.4, 4). Course items: Dot + countdown (picked).

function NavLink({
  to,
  icon,
  children,
  end,
  badge,
}: {
  to: string;
  icon?: ReactNode;
  children: ReactNode;
  end?: ReactNode;
  badge?: ReactNode;
}) {
  return (
    <Link
      to={to}
      activeOptions={{ exact: to === "/" }}
      className="flex h-8 items-center gap-2.5 rounded-[6px] px-2.5 text-text hover:bg-sunken data-[status=active]:bg-sunken data-[status=active]:font-medium [&_svg]:size-4 [&_svg]:text-muted"
    >
      {icon}
      <span className="min-w-0 flex-1 truncate">{children}</span>
      {end ? <span className="font-mono text-xs text-muted tnum">{end}</span> : null}
      {badge}
    </Link>
  );
}

function Invitations() {
  const { data } = useInvitations();
  const answer = useAnswerInvitation();
  if (!data?.length) return null;
  return (
    <div className="mt-3 flex flex-col gap-2">
      {data.map((i) => (
        <div key={i.courseId} className="flex flex-col gap-2 rounded-md border border-border bg-surface p-2.5">
          <span className="text-sm">
            {t("invite.shared", { who: i.invitedBy ?? "Someone", course: i.courseCode })
              .split(i.courseCode)
              .flatMap((part, n, all) =>
                n < all.length - 1 ? [part, <strong key={n}>{i.courseCode}</strong>] : [part],
              )}
          </span>
          <div className="flex gap-1.5">
            <Button
              size="sm"
              variant="primary"
              className="h-7 flex-1"
              disabled={answer.isPending}
              onClick={() => answer.mutate({ courseId: i.courseId, accept: true })}
            >
              {t("invite.accept")}
            </Button>
            <Button
              size="sm"
              className="h-7 flex-1"
              disabled={answer.isPending}
              onClick={() => answer.mutate({ courseId: i.courseId, accept: false })}
            >
              {t("invite.decline")}
            </Button>
          </div>
        </div>
      ))}
    </div>
  );
}

export function Sidebar({ onNavigate }: { onNavigate?: () => void }) {
  const courses = useCourses();
  const next = useNextExams(courses.data);
  const unread = useUnreadCount();
  const due = useDueCount();
  const path = useRouterState({ select: (s) => s.location.pathname });
  const live = (courses.data ?? []).filter(
    (c) => !c.trashedAt && c.mySubscription !== "Passed" && c.mySubscription !== "Archived",
  );

  return (
    <nav
      aria-label="Main"
      className="flex h-full flex-col gap-1 px-3 py-4"
      onClick={(e) => (e.target as HTMLElement).closest("a") && onNavigate?.()}
    >
      <Link to="/" className="flex items-center gap-2.5 px-2.5 pb-3 pt-1" aria-label="Crux home">
        <Mark size={24} />
        <span className="text-md font-semibold tracking-[-0.01em]">{t("app.name")}</span>
      </Link>
      <Button asChild variant="primary" className="mb-2">
        <Link to="/next">
          <ArrowRight aria-hidden="true" />
          {t("nav.takeMeThere")}
        </Link>
      </Button>
      <NavLink to="/" icon={<Home />}>
        {t("nav.home")}
      </NavLink>
      <NavLink to="/week" icon={<CalendarDays />}>
        {t("nav.week")}
      </NavLink>
      <NavLink to="/plan" icon={<CalendarRange />}>
        {t("nav.plan")}
      </NavLink>
      <NavLink to="/insight" icon={<CircleAlert />}>
        {t("nav.mistakes")}
      </NavLink>

      <Invitations />

      <span className="px-2.5 pb-1 pt-3 text-xs font-medium uppercase tracking-[0.06em] text-muted">
        {t("nav.courses")}
      </span>
      {live.map((c) => {
        const exam = next[c.id];
        return (
          <Link
            key={c.id}
            to="/c/$courseId"
            params={{ courseId: c.id }}
            className={cn(
              "flex h-8 items-center gap-2.5 rounded-[6px] px-2.5 hover:bg-sunken",
              path.startsWith(`/c/${c.id}`) && "bg-sunken font-medium",
            )}
          >
            <Dot color={c.color} />
            <span className="min-w-0 flex-1 truncate" title={c.name}>
              {c.code}
            </span>
            {exam ? <span className="font-mono text-xs text-muted tnum">{shortUntil(exam.startsAt)}</span> : null}
          </Link>
        );
      })}
      <Link
        to="/courses/new"
        className="flex h-8 items-center gap-2.5 rounded-[6px] px-2.5 text-muted hover:bg-sunken hover:text-text"
      >
        <Plus className="size-4" aria-hidden="true" />
        {t("nav.newCourse")}
      </Link>
      <Link to="/trash" className="flex h-7 items-center px-2.5 text-sm text-muted hover:text-text">
        {t("nav.trash")}
      </Link>

      <div className="mt-auto flex flex-col gap-0.5 pt-4">
        <NavLink to="/flash" icon={<Layers />} end={due.data ? `${due.data} due` : undefined}>
          {t("nav.flashcards")}
        </NavLink>
        <NavLink
          to="/inbox"
          icon={<Inbox />}
          badge={
            unread.data ? (
              <span className="inline-flex h-5 min-w-5 items-center justify-center rounded-full bg-primary px-1.5 text-xs font-medium text-primary-text tnum">
                {unread.data > 99 ? "99+" : unread.data}
                <span className="sr-only"> unread</span>
              </span>
            ) : null
          }
        >
          {t("nav.inbox")}
        </NavLink>
        <NavLink to="/settings" icon={<Settings />}>
          {t("nav.settings")}
        </NavLink>
        <div className="mt-2 border-t border-border pt-2">
          <AccountMenu />
        </div>
      </div>
    </nav>
  );
}

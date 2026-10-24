import { Link, Outlet, useNavigate, useRouterState } from "@tanstack/react-router";
import { ArrowRight, BookOpen, Inbox, Layers } from "lucide-react";
import { Dialog as D } from "radix-ui";
import { useEffect, useState } from "react";
import { AccountBanners } from "@/components/shell/banners";
import { CommandPalette } from "@/components/shell/command-palette";
import { Sidebar } from "@/components/shell/sidebar";
import { TopBar } from "@/components/shell/top-bar";
import { Kbd } from "@/components/ui/misc";
import { Dialog } from "@/components/ui/overlays";
import { useNotifyStream } from "@/features/notify/api";
import { startActivity } from "@/lib/activity";
import { typingIn } from "@/lib/keyboard";
import { t } from "@/lib/t";

const SHORTCUTS: [string, string][] = [
  ["⌘K / Ctrl K", "Search and go to"],
  ["g then c", "Go to your courses"],
  ["?", "Show these shortcuts"],
  ["1–9", "Mock: choose an option (Sure)"],
  ["Shift + 1–9", "Mock: choose, Unsure"],
  ["Alt + 1–9", "Mock: choose, Guess"],
  ["Space", "Mock: next question · Flashcards: show answer"],
  ["F", "Mock: flag the question"],
  ["1–4", "Flashcards: Again, Hard, Good, Easy"],
];

declare module "@tanstack/react-router" {
  interface StaticDataRouteOption {
    /** A full-screen task (taking a mock): no sidebar, top bar or bottom bar. */
    focus?: boolean;
  }
}

/** Signed-in layout (frontend-spec 4): sidebar, top bar, banners; on a phone a drawer + bottom bar. */
export function AppShell() {
  const [palette, setPalette] = useState(false);
  const [drawer, setDrawer] = useState(false);
  const [help, setHelp] = useState(false);
  const navigate = useNavigate();
  const focus = useRouterState({ select: (r) => r.matches.some((m) => m.staticData.focus) });
  useNotifyStream(true);
  useEffect(startActivity, []);

  useEffect(() => {
    let g = 0;
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPalette((o) => !o);
        return;
      }
      if (typingIn(e.target) || e.metaKey || e.ctrlKey || e.altKey) return;
      if (e.key === "?") {
        setHelp(true);
      } else if (e.key === "g") {
        g = Date.now();
      } else if (e.key === "c" && Date.now() - g < 1000) {
        void navigate({ to: "/" });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [navigate]);

  if (focus) {
    return (
      <main id="main" className="flex min-h-full flex-col">
        <Outlet />
      </main>
    );
  }

  return (
    <div className="flex min-h-full">
      <aside className="sticky top-0 hidden h-screen w-[248px] shrink-0 overflow-y-auto border-r border-border md:block">
        <Sidebar />
      </aside>

      <D.Root open={drawer} onOpenChange={setDrawer}>
        <D.Portal>
          <D.Overlay className="fixed inset-0 z-40 bg-black/30 md:hidden" />
          <D.Content
            aria-describedby={undefined}
            className="fixed inset-y-0 left-0 z-50 w-[280px] overflow-y-auto bg-bg shadow-raised md:hidden"
          >
            <D.Title className="sr-only">{t("nav.menu")}</D.Title>
            <Sidebar onNavigate={() => setDrawer(false)} />
          </D.Content>
        </D.Portal>
      </D.Root>

      <div className="flex min-w-0 flex-1 flex-col pb-16 md:pb-0">
        <TopBar onMenu={() => setDrawer(true)} onSearch={() => setPalette(true)} />
        <main id="main" className="mx-auto flex w-full max-w-[1200px] flex-1 flex-col gap-6 px-4 py-6 md:px-10">
          <AccountBanners />
          <Outlet />
        </main>
      </div>

      <nav
        aria-label="Quick"
        className="fixed inset-x-0 bottom-0 z-30 grid grid-cols-4 border-t border-border bg-surface pb-[env(safe-area-inset-bottom)] md:hidden"
      >
        {[
          { to: "/next", icon: ArrowRight, label: t("nav.takeMeThere") },
          { to: "/", icon: BookOpen, label: t("nav.courses") },
          { to: "/flash", icon: Layers, label: t("nav.flashcards") },
          { to: "/inbox", icon: Inbox, label: t("nav.inbox") },
        ].map(({ to, icon: Icon, label }) => (
          <Link
            key={to}
            to={to}
            activeOptions={{ exact: to === "/" }}
            className="flex h-14 flex-col items-center justify-center gap-0.5 text-xs text-muted data-[status=active]:text-text"
          >
            <Icon className="size-5" aria-hidden="true" strokeWidth={1.75} />
            {label}
          </Link>
        ))}
      </nav>

      <CommandPalette open={palette} onOpenChange={setPalette} />
      <Dialog open={help} onOpenChange={setHelp} title={t("shortcuts.title")}>
        <dl className="grid grid-cols-[auto_1fr] gap-x-6 gap-y-2">
          {SHORTCUTS.map(([k, what]) => (
            <div key={k} className="contents">
              <dt>
                <Kbd>{k}</Kbd>
              </dt>
              <dd className="m-0">{what}</dd>
            </div>
          ))}
        </dl>
      </Dialog>
    </div>
  );
}

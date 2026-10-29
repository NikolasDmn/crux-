import { createFileRoute, Link, Outlet } from "@tanstack/react-router";

// Settings (frontend-spec 4): one page per area under /settings, tabs like a course's. Storage and
// AI arrive with build steps 7 and 8.
export const Route = createFileRoute("/_app/settings")({ component: SettingsLayout });

const TABS = [
  { to: "/settings/profile", label: "Profile" },
  { to: "/settings/study", label: "Study" },
  { to: "/settings/notifications", label: "Notifications" },
  { to: "/settings/storage", label: "Storage" },
  { to: "/settings/ai", label: "AI" },
  { to: "/settings/account", label: "Account" },
] as const;

function SettingsLayout() {
  return (
    <div className="flex max-w-[760px] flex-col gap-5">
      <header className="flex flex-col gap-3">
        <h1 className="m-0 text-xl font-semibold tracking-[-0.01em]">Settings</h1>
        <nav
          aria-label="Settings"
          className="-mx-1 flex gap-5 overflow-x-auto overflow-y-hidden border-b border-border px-1"
        >
          {TABS.map((t) => (
            <Link
              key={t.to}
              to={t.to}
              className="-mb-px whitespace-nowrap border-b-2 border-transparent py-2 text-muted hover:text-text data-[status=active]:border-text data-[status=active]:font-medium data-[status=active]:text-text"
            >
              {t.label}
            </Link>
          ))}
        </nav>
      </header>
      <Outlet />
    </div>
  );
}

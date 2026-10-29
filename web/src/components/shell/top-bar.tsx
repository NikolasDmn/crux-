import { Menu as MenuIcon, Search } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Kbd } from "@/components/ui/misc";
import { t } from "@/lib/t";

const isMac = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform);

// The top bar is search (and the menu button on phones). The account menu sits at the foot of the
// sidebar; the theme is in Settings > Profile and Ctrl K.
export function TopBar({ onMenu, onSearch, title }: { onMenu: () => void; onSearch: () => void; title?: string }) {
  return (
    <header className="flex h-14 items-center gap-2 border-b border-border px-4 md:px-10">
      <Button variant="ghost" size="icon" className="-ml-2 md:hidden" aria-label={t("nav.menu")} onClick={onMenu}>
        <MenuIcon />
      </Button>
      <span className="min-w-0 flex-1 truncate text-sm text-muted md:hidden">{title}</span>
      <button
        type="button"
        aria-label={t("nav.search")}
        onClick={onSearch}
        className="flex h-9 items-center gap-2 rounded-md border border-border bg-surface px-3 text-sm text-muted hover:text-text md:w-full md:max-w-[480px]"
      >
        <Search className="size-4" aria-hidden="true" />
        <span className="hidden flex-1 text-left sm:inline">{t("nav.search")}</span>
        <Kbd className="hidden sm:inline">{isMac ? "⌘K" : "Ctrl K"}</Kbd>
      </button>
    </header>
  );
}

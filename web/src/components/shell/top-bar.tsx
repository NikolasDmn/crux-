import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { Menu as MenuIcon, Monitor, Moon, Search, Sun } from "lucide-react";
import { useState } from "react";
import { useAuth, useUser } from "@/app/auth";
import { Button } from "@/components/ui/button";
import { Kbd } from "@/components/ui/misc";
import {
  Menu,
  MenuContent,
  MenuItem,
  MenuLabel,
  MenuRadioGroup,
  MenuRadioItem,
  MenuSeparator,
  MenuTrigger,
  Tooltip,
} from "@/components/ui/overlays";
import { logout } from "@/features/auth/api";
import { applyTheme, getTheme, type Theme } from "@/lib/theme";
import { t } from "@/lib/t";

const isMac = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform);

export function ThemeMenu() {
  const [theme, setTheme] = useState<Theme>(getTheme);
  const Icon = theme === "dark" ? Moon : theme === "light" ? Sun : Monitor;
  return (
    <Menu>
      <Tooltip label={t("theme.label")}>
        <MenuTrigger asChild>
          <Button variant="ghost" size="icon" aria-label={`${t("theme.label")}: ${t(`theme.${theme}`)}`}>
            <Icon />
          </Button>
        </MenuTrigger>
      </Tooltip>
      <MenuContent align="end">
        <MenuLabel>{t("theme.label")}</MenuLabel>
        <MenuRadioGroup
          value={theme}
          onValueChange={(v) => {
            applyTheme(v as Theme);
            setTheme(v as Theme);
          }}
        >
          <MenuRadioItem value="system">{t("theme.system")}</MenuRadioItem>
          <MenuRadioItem value="light">{t("theme.light")}</MenuRadioItem>
          <MenuRadioItem value="dark">{t("theme.dark")}</MenuRadioItem>
        </MenuRadioGroup>
      </MenuContent>
    </Menu>
  );
}

function AvatarMenu() {
  const user = useUser();
  const { setUser } = useAuth();
  const qc = useQueryClient();
  const navigate = useNavigate();
  const initials =
    `${user.firstName[0] ?? ""}${user.lastName[0] ?? ""}`.toUpperCase() || user.username[0]?.toUpperCase();
  return (
    <Menu>
      <MenuTrigger asChild>
        <button
          type="button"
          aria-label={`Account: ${user.username}`}
          className="inline-flex size-8 items-center justify-center rounded-full bg-sunken text-sm font-medium"
        >
          {initials}
        </button>
      </MenuTrigger>
      <MenuContent align="end">
        <MenuLabel>{user.username}</MenuLabel>
        <MenuItem onSelect={() => void navigate({ to: "/settings" })}>{t("nav.settings")}</MenuItem>
        {user.role === "Admin" ? (
          <MenuItem onSelect={() => void navigate({ to: "/admin" })}>{t("nav.admin")}</MenuItem>
        ) : null}
        <MenuSeparator />
        <MenuItem
          onSelect={async () => {
            await logout();
            qc.clear();
            setUser(null);
            void navigate({ to: "/login" });
          }}
        >
          {t("nav.signOut")}
        </MenuItem>
      </MenuContent>
    </Menu>
  );
}

export function TopBar({ onMenu, onSearch, title }: { onMenu: () => void; onSearch: () => void; title?: string }) {
  return (
    <header className="flex h-14 items-center gap-2 border-b border-border px-4 md:px-10">
      <Button variant="ghost" size="icon" className="-ml-2 md:hidden" aria-label={t("nav.menu")} onClick={onMenu}>
        <MenuIcon />
      </Button>
      <span className="min-w-0 flex-1 truncate text-sm text-muted">{title}</span>
      <button
        type="button"
        onClick={onSearch}
        className="flex h-8 items-center gap-2 rounded-md border border-border bg-surface px-3 text-sm text-muted hover:text-text"
      >
        <Search className="size-4" aria-hidden="true" />
        <span className="hidden sm:inline">{t("nav.search")}</span>
        <Kbd className="hidden sm:inline">{isMac ? "⌘K" : "Ctrl K"}</Kbd>
      </button>
      <ThemeMenu />
      <AvatarMenu />
    </header>
  );
}

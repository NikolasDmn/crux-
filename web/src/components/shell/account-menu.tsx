import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { ChevronsUpDown, Monitor, Moon, Sun } from "lucide-react";
import { useState } from "react";
import { useAuth, useUser } from "@/app/auth";
import { Button } from "@/components/ui/button";
import { Card, SectionLabel } from "@/components/ui/card";
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
import { Segmented } from "@/components/ui/segmented";
import { logout } from "@/features/auth/api";
import { applyTheme, getTheme, type Theme } from "@/lib/theme";
import { t } from "@/lib/t";

/** At the foot of the sidebar: who's signed in, with Settings, Admin and Sign out. */
export function AccountMenu() {
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
          className="flex h-10 w-full items-center gap-2.5 rounded-[6px] px-2 text-left hover:bg-sunken"
        >
          <span className="inline-flex size-7 shrink-0 items-center justify-center rounded-full bg-sunken text-xs font-medium">
            {initials}
          </span>
          <span className="flex min-w-0 flex-1 flex-col leading-tight">
            <span className="truncate text-sm font-medium">
              {user.firstName} {user.lastName}
            </span>
            <span className="truncate text-xs text-muted">@{user.username}</span>
          </span>
          <ChevronsUpDown className="size-4 text-muted" aria-hidden="true" />
        </button>
      </MenuTrigger>
      <MenuContent align="start" side="top">
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

/** Settings > Profile: System, Light or Dark (also in Ctrl K). */
export function AppearanceCard() {
  const [theme, setTheme] = useState<Theme>(getTheme);
  return (
    <Card className="flex flex-col gap-3">
      <SectionLabel>Appearance</SectionLabel>
      <Segmented<Theme>
        label={t("theme.label")}
        size="sm"
        className="self-start"
        value={theme}
        onChange={(v) => {
          applyTheme(v);
          setTheme(v);
        }}
        options={[
          { value: "system", label: t("theme.system") },
          { value: "light", label: t("theme.light") },
          { value: "dark", label: t("theme.dark") },
        ]}
      />
    </Card>
  );
}

/** The compact theme switch (the UI gallery uses it). */
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

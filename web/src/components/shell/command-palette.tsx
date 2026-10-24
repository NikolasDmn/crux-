import { useNavigate } from "@tanstack/react-router";
import { Command } from "cmdk";
import {
  ArrowRight,
  CalendarDays,
  CalendarRange,
  CircleAlert,
  Home,
  Inbox,
  Layers,
  Monitor,
  Moon,
  Plus,
  Settings,
  Sun,
  Trash2,
} from "lucide-react";
import { Dialog as D } from "radix-ui";
import type { ReactNode } from "react";
import { Dot } from "@/components/ui/misc";
import { useCourses } from "@/features/courses/api";
import { applyTheme } from "@/lib/theme";
import { t } from "@/lib/t";

// ⌘/Ctrl+K (frontend-spec 2.6): go to a course or page; "Take me there"; switch theme.

function Item({
  onSelect,
  icon,
  children,
  value,
}: {
  onSelect: () => void;
  icon: ReactNode;
  children: ReactNode;
  value: string;
}) {
  return (
    <Command.Item
      value={value}
      onSelect={onSelect}
      className="flex h-9 cursor-default items-center gap-2.5 rounded-sm px-2.5 data-[selected=true]:bg-sunken [&_svg]:size-4 [&_svg]:text-muted"
    >
      {icon}
      {children}
    </Command.Item>
  );
}

export function CommandPalette({ open, onOpenChange }: { open: boolean; onOpenChange: (o: boolean) => void }) {
  const navigate = useNavigate();
  const courses = useCourses();
  const go = (to: string, params?: Record<string, string>) => {
    onOpenChange(false);
    void navigate({ to, params } as never);
  };
  const theme = (v: "system" | "light" | "dark") => {
    applyTheme(v);
    onOpenChange(false);
  };
  return (
    <D.Root open={open} onOpenChange={onOpenChange}>
      <D.Portal>
        <D.Overlay className="fixed inset-0 z-40 bg-black/30" />
        <D.Content
          aria-describedby={undefined}
          className="fixed left-1/2 top-[12vh] z-50 w-[min(560px,calc(100vw-32px))] -translate-x-1/2 overflow-hidden rounded-lg border border-border bg-raised shadow-raised"
        >
          <D.Title className="sr-only">{t("nav.search")}</D.Title>
          <Command label={t("nav.search")} className="flex flex-col">
            <Command.Input
              autoFocus
              placeholder={t("palette.placeholder")}
              className="h-12 border-b border-border bg-transparent px-4 text-md outline-none placeholder:text-muted"
            />
            <Command.List className="max-h-[min(60vh,420px)] overflow-y-auto p-1.5">
              <Command.Empty className="px-3 py-6 text-center text-muted">{t("palette.empty")}</Command.Empty>
              <Command.Group heading={t("palette.go")}>
                <Item value="take me there next" onSelect={() => go("/next")} icon={<ArrowRight />}>
                  {t("nav.takeMeThere")}
                </Item>
                <Item value="home" onSelect={() => go("/")} icon={<Home />}>
                  {t("nav.home")}
                </Item>
                <Item value="this week" onSelect={() => go("/week")} icon={<CalendarDays />}>
                  {t("nav.week")}
                </Item>
                <Item value="plan calendar" onSelect={() => go("/plan")} icon={<CalendarRange />}>
                  {t("nav.plan")}
                </Item>
                <Item value="mistakes slips weak spots" onSelect={() => go("/insight")} icon={<CircleAlert />}>
                  {t("nav.mistakes")}
                </Item>
                <Item value="flashcards" onSelect={() => go("/flash")} icon={<Layers />}>
                  {t("nav.flashcards")}
                </Item>
                <Item value="inbox notifications" onSelect={() => go("/inbox")} icon={<Inbox />}>
                  {t("nav.inbox")}
                </Item>
                <Item value="settings" onSelect={() => go("/settings")} icon={<Settings />}>
                  {t("nav.settings")}
                </Item>
                <Item value="new course" onSelect={() => go("/courses/new")} icon={<Plus />}>
                  {t("nav.newCourse")}
                </Item>
                <Item value="trash" onSelect={() => go("/trash")} icon={<Trash2 />}>
                  {t("nav.trash")}
                </Item>
              </Command.Group>
              {courses.data?.length ? (
                <Command.Group heading={t("nav.courses")}>
                  {courses.data
                    .filter((c) => !c.trashedAt)
                    .map((c) => (
                      <Item
                        key={c.id}
                        value={`${c.code} ${c.name}`}
                        onSelect={() => go("/c/$courseId", { courseId: c.id })}
                        icon={<Dot color={c.color} />}
                      >
                        <span className="font-medium">{c.code}</span>
                        <span className="text-muted">{c.name}</span>
                      </Item>
                    ))}
                </Command.Group>
              ) : null}
              <Command.Group heading={t("palette.actions")}>
                <Item value="theme system" onSelect={() => theme("system")} icon={<Monitor />}>
                  {t("theme.label")}: {t("theme.system")}
                </Item>
                <Item value="theme light" onSelect={() => theme("light")} icon={<Sun />}>
                  {t("theme.label")}: {t("theme.light")}
                </Item>
                <Item value="theme dark" onSelect={() => theme("dark")} icon={<Moon />}>
                  {t("theme.label")}: {t("theme.dark")}
                </Item>
              </Command.Group>
            </Command.List>
          </Command>
        </D.Content>
      </D.Portal>
    </D.Root>
  );
}

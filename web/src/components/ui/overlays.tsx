import { X } from "lucide-react";
import { Dialog as D, DropdownMenu as M, Tooltip as T } from "radix-ui";
import type { ComponentProps, ReactNode } from "react";
import { cn } from "@/lib/cn";

// Raised layers (frontend-spec 3.2): --surface-raised + --shadow; focus trapping comes from Radix.

const raised = "rounded-lg border border-border bg-raised text-text shadow-raised";

export function Dialog({
  open,
  onOpenChange,
  title,
  description,
  children,
  footer,
  className,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: ReactNode;
  description?: ReactNode;
  children?: ReactNode;
  footer?: ReactNode;
  className?: string;
}) {
  return (
    <D.Root open={open} onOpenChange={onOpenChange}>
      <D.Portal>
        <D.Overlay className="fixed inset-0 z-40 bg-black/30" />
        <D.Content
          className={cn(
            raised,
            "fixed left-1/2 top-1/2 z-50 flex max-h-[85vh] w-[min(480px,calc(100vw-32px))] -translate-x-1/2 -translate-y-1/2 flex-col gap-4 overflow-auto p-6",
            className,
          )}
        >
          <div className="flex items-start gap-4">
            <div className="flex min-w-0 flex-1 flex-col gap-1">
              <D.Title className="text-lg font-semibold">{title}</D.Title>
              {description ? <D.Description className="text-muted">{description}</D.Description> : null}
            </div>
            <D.Close className="-m-1 rounded-md p-1 text-muted hover:text-text" aria-label="Close">
              <X className="size-4" />
            </D.Close>
          </div>
          {children}
          {footer ? <div className="flex flex-wrap justify-end gap-2">{footer}</div> : null}
        </D.Content>
      </D.Portal>
    </D.Root>
  );
}

export const Menu = M.Root;
export const MenuTrigger = M.Trigger;

export function MenuContent({ className, ...p }: ComponentProps<typeof M.Content>) {
  return (
    <M.Portal>
      <M.Content sideOffset={6} className={cn(raised, "z-50 min-w-48 p-1", className)} {...p} />
    </M.Portal>
  );
}

export function MenuItem({ className, ...p }: ComponentProps<typeof M.Item>) {
  return (
    <M.Item
      className={cn(
        "flex h-8 cursor-default select-none items-center gap-2 rounded-sm px-2 outline-none data-[highlighted]:bg-sunken [&_svg]:size-4",
        className,
      )}
      {...p}
    />
  );
}

export const MenuSeparator = () => <M.Separator className="my-1 h-px bg-border" />;
export const MenuLabel = ({ children }: { children: ReactNode }) => (
  <M.Label className="px-2 py-1.5 text-xs font-medium uppercase tracking-[0.06em] text-muted">{children}</M.Label>
);
export const MenuRadioGroup = M.RadioGroup;
export function MenuRadioItem({ className, children, ...p }: ComponentProps<typeof M.RadioItem>) {
  return (
    <M.RadioItem
      className={cn(
        "flex h-8 cursor-default select-none items-center gap-2 rounded-sm px-2 outline-none data-[highlighted]:bg-sunken",
        className,
      )}
      {...p}
    >
      <span className="inline-flex size-4 items-center justify-center">
        <M.ItemIndicator>
          <span className="size-1.5 rounded-full bg-text" />
        </M.ItemIndicator>
      </span>
      {children}
    </M.RadioItem>
  );
}

export function Tooltip({ label, children }: { label: ReactNode; children: ReactNode }) {
  return (
    <T.Root delayDuration={400}>
      <T.Trigger asChild>{children}</T.Trigger>
      <T.Portal>
        <T.Content
          sideOffset={6}
          className="z-50 max-w-64 rounded-md bg-primary px-2.5 py-1.5 text-sm text-primary-text"
        >
          {label}
        </T.Content>
      </T.Portal>
    </T.Root>
  );
}

export const TooltipProvider = T.Provider;

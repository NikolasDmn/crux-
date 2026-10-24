import type { ReactNode } from "react";
import { Mark } from "@/components/ui/misc";

/** The signed-out pages: one centred column under the mark. */
export function AuthLayout({ title, children, footer }: { title: string; children: ReactNode; footer?: ReactNode }) {
  return (
    <main className="flex min-h-full items-start justify-center px-4 py-[12vh]">
      <div className="flex w-full max-w-[400px] flex-col gap-6">
        <div className="flex flex-col items-center gap-4 text-center">
          <Mark size={40} />
          <h1 className="m-0 text-xl font-semibold tracking-[-0.01em]">{title}</h1>
        </div>
        <div className="flex flex-col gap-5 rounded-lg border border-border bg-surface p-6">{children}</div>
        {footer ? <div className="flex flex-col items-center gap-2 text-sm text-muted">{footer}</div> : null}
      </div>
    </main>
  );
}

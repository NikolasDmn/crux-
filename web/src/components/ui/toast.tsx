import { Toast as R } from "radix-ui";
import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from "react";
import { cn } from "@/lib/cn";

// Picked: Ink toast (frontend-spec 3.3): bottom-left on desktop, bottom-centre on a phone; 6 s,
// paused on hover and focus (Radix); an outlined Undo in the same colour.

export interface ToastInput {
  message: string;
  /** e.g. Undo. Runs once; the toast closes. */
  action?: { label: string; run: () => void };
  tone?: "default" | "danger";
}

interface Item extends ToastInput {
  id: number;
}

const Ctx = createContext<(t: ToastInput) => void>(() => {});

export function useToast(): (t: ToastInput) => void {
  return useContext(Ctx);
}

export function ToastProvider({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<Item[]>([]);
  const show = useCallback(
    (t: ToastInput) => setItems((xs) => [...xs.slice(-2), { ...t, id: Date.now() + Math.random() }]),
    [],
  );
  const value = useMemo(() => show, [show]);
  return (
    <Ctx.Provider value={value}>
      <R.Provider duration={6000} swipeDirection="down">
        {children}
        {items.map((t) => (
          <R.Root
            key={t.id}
            onOpenChange={(open) => !open && setItems((xs) => xs.filter((x) => x.id !== t.id))}
            className={cn(
              "flex items-center gap-4 rounded-[10px] px-4 py-3 shadow-raised",
              t.tone === "danger" ? "bg-danger text-white" : "bg-primary text-primary-text",
            )}
          >
            <R.Description className="min-w-0 flex-1">{t.message}</R.Description>
            {t.action ? (
              <R.Action asChild altText={t.action.label}>
                <button
                  type="button"
                  onClick={t.action.run}
                  className="h-7 shrink-0 rounded-[6px] border border-current px-2.5 text-sm font-medium"
                >
                  {t.action.label}
                </button>
              </R.Action>
            ) : null}
          </R.Root>
        ))}
        <R.Viewport className="fixed bottom-[max(16px,env(safe-area-inset-bottom))] left-1/2 z-50 flex w-[min(420px,calc(100vw-32px))] -translate-x-1/2 flex-col gap-2 outline-none sm:left-6 sm:translate-x-0" />
      </R.Provider>
    </Ctx.Provider>
  );
}

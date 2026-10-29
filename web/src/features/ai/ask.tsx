import { Sparkles } from "lucide-react";
import { createContext, lazy, Suspense, useCallback, useContext, useMemo, useState, type ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { Tooltip } from "@/components/ui/overlays";
import { useAi } from "./api";
import type { AskTarget } from "./lib/chat";
import { reasonText } from "./lib/format";

// Ask AI's always-loaded half: the provider every page shares and the "Ask AI" button. The panel
// (./ask-panel.tsx) loads on the first open.

const AskPanel = lazy(() => import("./ask-panel").then((m) => ({ default: m.AskPanel })));

const Ctx = createContext<{ open: (t: AskTarget) => void } | null>(null);

export function useAsk() {
  const c = useContext(Ctx);
  if (!c) throw new Error("useAsk outside AskProvider");
  return c;
}

export function AskProvider({ children }: { children: ReactNode }) {
  const [target, setTarget] = useState<AskTarget | null>(null);
  const [n, setN] = useState(0);
  const open = useCallback((t: AskTarget) => {
    setTarget(t);
    setN((x) => x + 1);
  }, []);
  const value = useMemo(() => ({ open }), [open]);
  return (
    <Ctx.Provider value={value}>
      {children}
      {target ? (
        <Suspense fallback={null}>
          <AskPanel key={n} target={target} onClose={() => setTarget(null)} />
        </Suspense>
      ) : null}
    </Ctx.Provider>
  );
}

/** "Ask AI" for a page (2.5): hidden while AI is off or unreachable, disabled with the reason when
 *  blocked here. */
export function AskButton({ target, size = "sm" }: { target: AskTarget; size?: "sm" | "md" }) {
  const gate = useAi("MaterialChat", target.courseId);
  const { open } = useAsk();
  if (!gate.visible) return null;
  const why = gate.allowed ? null : reasonText(gate.reason, target.courseId);
  const button = (
    <Button size={size} disabled={!gate.allowed} onClick={() => open(target)}>
      <Sparkles aria-hidden="true" />
      Ask AI
    </Button>
  );
  return why ? (
    <Tooltip label={why.text}>
      <span tabIndex={0}>{button}</span>
    </Tooltip>
  ) : (
    button
  );
}

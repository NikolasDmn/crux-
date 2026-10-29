import { TexMath } from "@/components/math";
import { linePieces } from "../lib/filters";

/** A question prompt or card side on one line, its inline maths rendered (lists, pickers). */
export function PromptLine({ md, max, empty = "" }: { md: string; max?: number; empty?: string }) {
  const pieces = linePieces(md, max);
  if (!pieces.length) return <>{empty}</>;
  return <>{pieces.map((p, i) => ("tex" in p ? <TexMath key={i} tex={p.tex} /> : <span key={i}>{p.text}</span>))}</>;
}

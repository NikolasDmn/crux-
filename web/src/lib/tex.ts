// Maths before it reaches a renderer (frontend-spec 2.4). Two cheap fixes catch most of what KaTeX
// rejects in real course material: LaTeX's \( \) and \[ \] delimiters (AI output uses them), and
// display-only environments written where KaTeX can't take them.

/** `\(x\)` to `$x$` and `\[x\]` to `$$x$$`, outside code. A lone `\(` stays as written. */
export function normalizeDelimiters(md: string): string {
  if (!md.includes("\\(") && !md.includes("\\[")) return md;
  // Split off fenced blocks and inline code; only the other parts change.
  const parts = md.split(/(```[\s\S]*?```|~~~[\s\S]*?~~~|`[^`\n]*`)/g);
  return parts
    .map((p, i) =>
      i % 2
        ? p
        : p
            .replace(/\\\[([\s\S]+?)\\\]/g, (_, m: string) => `\n$$\n${m.trim()}\n$$\n`)
            .replace(/\\\(([^\n]+?)\\\)/g, (_, m: string) => `$${m.trim()}$`),
    )
    .join("");
}

const ALIGN = /\\begin\{(align|alignat|flalign|eqnarray|multline|gather)\*?\}([\s\S]*?)\\end\{\1\*?\}/g;

/** Rewrites to try when KaTeX rejects `tex`; empty when nothing applies. */
export function repairs(tex: string): string[] {
  let t = tex
    // MathJax-only; KaTeX has the packages built in or not at all.
    .replace(/\\require\{[^}]*\}/g, "")
    // Numbering and labels mean nothing on a card.
    .replace(/\\(?:label|tag\*?)\{[^}]*\}|\\(?:nonumber|notag)\b/g, "")
    // Numbered display environments: their unnumbered "-ed" forms work anywhere.
    .replace(ALIGN, (_, env: string, body: string) =>
      env === "gather" || env === "multline"
        ? `\\begin{gathered}${body}\\end{gathered}`
        : `\\begin{aligned}${env === "eqnarray" ? body.replace(/&\s*([=<>]|\\[a-z]+)\s*&/g, "&$1 ") : body}\\end{aligned}`,
    )
    .replace(/\\begin\{(equation|displaymath)\*?\}([\s\S]*?)\\end\{\1\*?\}/g, "$2")
    .trim();
  if (t === tex.trim()) return [];
  if (!t) t = "{}";
  return [t];
}

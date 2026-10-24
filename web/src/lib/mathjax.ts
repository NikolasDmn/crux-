// MathJax 4 (TeX in, SVG out), lazy-loaded (frontend-spec 2.4, 6). SVG output draws glyphs as
// paths, so no web fonts are fetched (CSP default-src 'self'). The `html` TeX package (\href,
// \class, \style, \cssId) is left out: maths comes from users and AI, so it can't add links or styles.

import { mathjax } from "@mathjax/src/js/mathjax.js";
import { TeX } from "@mathjax/src/js/input/tex.js";
import { SVG } from "@mathjax/src/js/output/svg.js";
import { liteAdaptor } from "@mathjax/src/js/adaptors/liteAdaptor.js";
import { RegisterHTMLHandler } from "@mathjax/src/js/handlers/html.js";
import "@mathjax/src/js/input/tex/base/BaseConfiguration.js";
import "@mathjax/src/js/input/tex/ams/AmsConfiguration.js";
import "@mathjax/src/js/input/tex/newcommand/NewcommandConfiguration.js";
import "@mathjax/src/js/input/tex/boldsymbol/BoldsymbolConfiguration.js";
import "@mathjax/src/js/input/tex/braket/BraketConfiguration.js";
import "@mathjax/src/js/input/tex/cancel/CancelConfiguration.js";
import "@mathjax/src/js/input/tex/cases/CasesConfiguration.js";
import "@mathjax/src/js/input/tex/color/ColorConfiguration.js";
import "@mathjax/src/js/input/tex/mathtools/MathtoolsConfiguration.js";
import { MhchemReplacements } from "@mathjax/src/js/input/tex/mhchem/MhchemConfiguration.js";
import "@mathjax/src/js/input/tex/physics/PhysicsConfiguration.js";
import "@mathjax/src/js/input/tex/textmacros/TextMacrosConfiguration.js";
import "@mathjax/src/js/input/tex/units/UnitsConfiguration.js";
import "@mathjax/src/js/input/tex/gensymb/GensymbConfiguration.js";
import "@mathjax/src/js/input/tex/upgreek/UpgreekConfiguration.js";
import { MathJaxTexFont } from "@mathjax/mathjax-tex-font/js/svg.js";

// mhchem swaps its arrows and bonds for private-use glyphs that only the bigger default font has;
// without the swaps it writes standard TeX arrows, which the TeX font draws.
MhchemReplacements.clear();

const adaptor = liteAdaptor();
RegisterHTMLHandler(adaptor);

const doc = mathjax.document("", {
  InputJax: new TeX({
    packages: [
      "base",
      "ams",
      "newcommand",
      "boldsymbol",
      "braket",
      "cancel",
      "cases",
      "color",
      "mathtools",
      "mhchem",
      "physics",
      "textmacros",
      "units",
      "gensymb",
      "upgreek",
    ],
    formatError: (_jax: unknown, err: Error) => {
      throw err;
    },
  }),
  // No font cache: shared <defs> ids would clash between separately converted expressions.
  OutputJax: new SVG({ fontData: MathJaxTexFont, fontCache: "none" }),
});

/** TeX to an SVG string; throws on a TeX error (an unknown macro included). */
export function texToSvg(tex: string, display: boolean): string {
  const node = doc.convert(tex, { display });
  return adaptor.outerHTML(node);
}

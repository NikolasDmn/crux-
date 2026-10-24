// Stands in for MathJax's default SVG font (New Computer Modern, about 1 MB of glyph paths) so
// only the smaller TeX font is bundled; vite.config.ts aliases `#default-font/svg/default.js` here.
import { MathJaxTexFont } from "@mathjax/mathjax-tex-font/js/svg.js";

export const Font = { fontName: "mathjax-tex", DefaultFont: MathJaxTexFont };

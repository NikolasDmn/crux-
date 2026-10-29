import { useWindowVirtualizer } from "@tanstack/react-virtual";
import { useLayoutEffect, useRef, useState } from "react";

/** Spec 6: lists over this many rows render only what's near the screen. */
export const VIRTUAL_AFTER = 100;

/**
 * Rows of a list or table that scrolls with the page. Up to `VIRTUAL_AFTER` rows it's every index;
 * past that, only the rows near the viewport, with `before`/`after` heights for spacer rows (a
 * `<tr>` or `<li>`) so the scrollbar and table layout stay right. Put `ref` on the list element and
 * `measure` (with `data-index`) on each row so rows of any height measure themselves.
 */
export function useVirtualRows<E extends HTMLElement>(count: number, estimate: number) {
  const ref = useRef<E>(null);
  const [margin, setMargin] = useState(0);
  const on = count > VIRTUAL_AFTER;
  useLayoutEffect(() => {
    if (on && ref.current) setMargin(ref.current.getBoundingClientRect().top + window.scrollY);
  }, [on]);
  const v = useWindowVirtualizer({
    count: on ? count : 0,
    estimateSize: () => estimate,
    overscan: 12,
    scrollMargin: margin,
  });
  if (!on) {
    return { ref, indices: Array.from({ length: count }, (_, i) => i), before: 0, after: 0, measure: undefined };
  }
  const items = v.getVirtualItems();
  const first = items[0];
  const last = items[items.length - 1];
  return {
    ref,
    indices: items.map((i) => i.index),
    before: first ? first.start - margin : 0,
    after: last ? v.getTotalSize() - (last.end - margin) : 0,
    measure: v.measureElement,
  };
}

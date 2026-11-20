/** Keep the DOM canvas well below browser layout-height limits; row IDs stay logical. */
export const ROW_CANVAS_PX = 1_000_000;

export interface RowScroll {
  /** Logical first row represented by the current physical scroll canvas. */
  originRow: number;
  /** The actual scrollTop inside that canvas, in pixels. */
  scrollTop: number;
}

/** Rows are fixed-height. viewportHeight includes the fixed column-header band. */
export function segmentedRows(
  rowCount: number,
  rowHeight: number,
  viewportHeight: number,
  headerHeight: number,
  frozenRows: number,
) {
  const frozenHeight = frozenRows * rowHeight;
  const bodyHeight = Math.max(0, viewportHeight - headerHeight);
  // Align both canvas and segment origin to whole rows. In the final segment,
  // physical and logical bottom clamps then agree exactly.
  const canvasRows = Math.min(rowCount, Math.max(
    Math.floor((ROW_CANVAS_PX - headerHeight) / rowHeight),
    Math.ceil(bodyHeight / rowHeight) + 1,
  ));
  const canvasHeight = canvasRows * rowHeight + headerHeight;
  const maxOriginRow = rowCount - canvasRows;
  const maxScrollTop = Math.max(0, canvasHeight - viewportHeight);
  const maxLogicalTop = maxOriginRow * rowHeight + maxScrollTop;
  const guard = maxScrollTop / 4;

  const logicalScrollTop = (state: RowScroll): number =>
    Math.max(0, Math.min(maxLogicalTop, state.originRow * rowHeight + state.scrollTop));

  // Center targets away from segment boundaries unless the global end/start
  // demands it. Preserve the fractional physical scroll offset exactly.
  const center = (logicalTop: number): RowScroll => {
    const originRow = Math.min(maxOriginRow, Math.max(0, Math.round(
      (logicalTop - maxScrollTop / 2) / rowHeight,
    )));
    return {originRow, scrollTop: logicalTop - originRow * rowHeight};
  };

  const place = (x: number, currentOrigin: number): RowScroll => {
    const logicalTop = Math.max(0, Math.min(maxLogicalTop, x));
    const originRow = Math.max(0, Math.min(maxOriginRow, currentOrigin));
    const scrollTop = logicalTop - originRow * rowHeight;
    if (scrollTop < 0 || scrollTop > maxScrollTop
      || (originRow > 0 && scrollTop < guard)
      || (originRow < maxOriginRow && scrollTop > maxScrollTop - guard)) {
      return center(logicalTop);
    }
    return {originRow, scrollTop};
  };

  return {
    canvasHeight,
    maxScrollTop,
    logicalScrollTop,
    /** Consume a native scroll position and return the same logical location,
     * possibly rebased. Apply a changed origin/scrollTop together before paint. */
    fromNative(originRow: number, scrollTop: number): RowScroll {
      return place(originRow * rowHeight + scrollTop, originRow);
    },
    /** Move to a logical row, while leaving already visible and pinned rows alone. */
    reveal(row: number, state: RowScroll): RowScroll {
      if (row < frozenRows || row >= rowCount || bodyHeight <= frozenHeight) return state;
      const old = logicalScrollTop(state);
      const top = row * rowHeight;
      const scrollingHeight = bodyHeight - frozenHeight;
      const next = rowHeight > scrollingHeight || top < old + frozenHeight
        ? top - frozenHeight
        : top + rowHeight > old + bodyHeight ? top + rowHeight - bodyHeight : old;
      return place(next, state.originRow);
    },
    /** Position inside the physical DOM canvas. Pinned rows follow the
     * physical scroll, independent of segment origin. */
    rowTop(row: number, state: RowScroll): number {
      return headerHeight + row * rowHeight
        + (row < frozenRows ? state.scrollTop : -state.originRow * rowHeight);
    },
    /** Bounds stay in worksheet coordinates for API requests, never in pixels. */
    visible(state: RowScroll, overscan: number) {
      const visibleFrozenRows = Math.min(frozenRows, Math.ceil(bodyHeight / rowHeight));
      if (!rowCount || bodyHeight <= frozenHeight) {
        // Match Grid's existing frozen-only viewport request convention.
        return {startRow:0, endRow:0, visibleFrozenRows};
      }
      const top = logicalScrollTop(state);
      return {
        startRow: Math.min(rowCount - 1, Math.max(0, Math.floor((top + frozenHeight) / rowHeight) - overscan)),
        endRow: Math.min(rowCount - 1, Math.ceil((top + bodyHeight) / rowHeight) - 1 + overscan),
        visibleFrozenRows,
      };
    },
  };
}

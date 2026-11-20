import type {ViewportResponse} from '../types';
import type {LoadedRange} from './frozenViewport';

export interface ViewBounds {
  startRow: number;
  startCol: number;
  endRow: number;
  endCol: number;
  pinnedRows: number;
  pinnedColumns: number;
}
type ViewExtent = {total_rows: number; total_cols: number};

const MIB = 1024 * 1024;
const MAX_FETCH_CELLS = 20_000;
const MAX_WINDOWS = 8;
const DEFAULT_BYTES_PER_CELL = 256;

export function viewportBudget(deviceMemoryGiB?: number): number {
  if (!Number.isFinite(deviceMemoryGiB) || deviceMemoryGiB! <= 0) return 4 * MIB;
  return Math.max(1, Math.min(8, deviceMemoryGiB!)) * MIB;
}

function toRange(bounds: ViewBounds, extent?: ViewExtent): LoadedRange {
  return {
    start_row: bounds.startRow, start_col: bounds.startCol,
    end_row: Math.min(bounds.endRow, (extent?.total_rows ?? Infinity) - 1),
    end_col: Math.min(bounds.endCol, (extent?.total_cols ?? Infinity) - 1),
  };
}

// The body, top strip, left strip and corner can be far apart in a frozen sheet.
function requiredRanges(bounds: ViewBounds, extent?: ViewExtent): LoadedRange[] {
  const body = toRange(bounds, extent);
  if (body.start_row > body.end_row || body.start_col > body.end_col) return [];
  const rows = Math.min(bounds.pinnedRows, extent?.total_rows ?? Infinity);
  const columns = Math.min(bounds.pinnedColumns, extent?.total_cols ?? Infinity);
  const ranges = [body];
  if (rows > 0) ranges.push({...body, start_row: 0, end_row: rows - 1});
  if (columns > 0) ranges.push({...body, start_col: 0, end_col: columns - 1});
  if (rows > 0 && columns > 0) ranges.push({start_row: 0, end_row: rows - 1, start_col: 0, end_col: columns - 1});
  return ranges;
}

// Check rectangular coverage against the actual sparse panes, not the bounding box
// or returned cells (an unpopulated cell inside a loaded range is a valid blank).
function rangeCovered(target: LoadedRange, ranges: LoadedRange[]): boolean {
  const rowEdges = [target.start_row, target.end_row + 1];
  for (const range of ranges) {
    if (range.start_row > target.start_row && range.start_row <= target.end_row) rowEdges.push(range.start_row);
    if (range.end_row >= target.start_row && range.end_row < target.end_row) rowEdges.push(range.end_row + 1);
  }
  rowEdges.sort((a,b) => a-b);
  for (const row of rowEdges.slice(0,-1)) {
    let col = target.start_col;
    const overlapping = ranges.filter(r => r.start_row <= row && r.end_row >= row).sort((a,b) => a.start_col - b.start_col);
    for (const range of overlapping) {
      if (range.start_col > col) return false;
      col = Math.max(col, range.end_col + 1);
      if (col > target.end_col) break;
    }
    if (col <= target.end_col) return false;
  }
  return true;
}

export function viewportCovers(view: ViewportResponse, bounds: ViewBounds): boolean {
  const needed = requiredRanges(bounds, view);
  return needed.length > 0 && needed.every(range => rangeCovered(range, view.loaded_ranges ?? [view]));
}

export function requestCovers(request: ViewBounds, bounds: ViewBounds): boolean {
  return request.startRow <= bounds.startRow && request.endRow >= bounds.endRow
    && request.startCol <= bounds.startCol && request.endCol >= bounds.endCol
    && request.pinnedRows >= bounds.pinnedRows && request.pinnedColumns >= bounds.pinnedColumns;
}

function paneArea(bounds: ViewBounds): number {
  const span = (start: number, end: number, pinned: number) =>
    end - start + 1 + Math.min(pinned, start) + Math.max(0, pinned - end - 1);
  return span(bounds.startRow, bounds.endRow, bounds.pinnedRows)
    * span(bounds.startCol, bounds.endCol, bounds.pinnedColumns);
}

export function bufferedViewport(bounds: ViewBounds, previous: ViewBounds | undefined,
  budget: number, bytesPerCell: number, extent?: ViewExtent): ViewBounds {
  const targetCells = Math.min(MAX_FETCH_CELLS,
    Math.max(paneArea(bounds), Math.floor(budget / 3 / Math.max(DEFAULT_BYTES_PER_CELL, bytesPerCell))));
  const rows = Math.min(44, Math.ceil((bounds.endRow - bounds.startRow + 1) * 1.1));
  const columns = Math.min(12, Math.ceil((bounds.endCol - bounds.startCol + 1) * 0.7));
  const rowForward = !previous || bounds.startRow >= previous.startRow;
  const colForward = !previous || bounds.startCol >= previous.startCol;
  for (const scale of [1, 0.75, 0.5, 0.25, 0]) {
    const before = (n: number, forward: boolean) => Math.floor(n * scale * (forward ? 0.2 : 0.8));
    const after = (n: number, forward: boolean) => Math.ceil(n * scale * (forward ? 0.8 : 0.2));
    const request = {
      startRow: Math.max(0, bounds.startRow - before(rows, rowForward)),
      startCol: Math.max(0, bounds.startCol - before(columns, colForward)),
      endRow: Math.min(bounds.endRow + after(rows, rowForward), (extent?.total_rows ?? Infinity) - 1),
      endCol: Math.min(bounds.endCol + after(columns, colForward), (extent?.total_cols ?? Infinity) - 1),
      pinnedRows: bounds.pinnedRows, pinnedColumns: bounds.pinnedColumns,
    };
    if (paneArea(request) <= targetCells && request.endRow >= bounds.startRow && request.endCol >= bounds.startCol) return request;
  }
  return bounds;
}

// Conservative JS payload estimates without allocating a second serialized copy.
function retainedBytes(value: unknown): number {
  if (typeof value === 'string') return 32 + value.length * 2;
  if (typeof value !== 'object' || value === null) return 16;
  let bytes = 64;
  for (const key in value) {
    if (!Object.prototype.hasOwnProperty.call(value,key)) continue;
    bytes += 32 + key.length * 2 + retainedBytes((value as Record<string,unknown>)[key]);
  }
  return bytes;
}

export class ViewportCache {
  private entries: {view: ViewportResponse; bytes: number}[] = [];
  private bytes = 0;
  private density = DEFAULT_BYTES_PER_CELL;
  private extent?: ViewExtent;
  private generation?: number;

  readonly budget: number;
  constructor(budget: number) { this.budget = budget; }
  clear() {
    this.entries = [];
    this.bytes = 0;
    this.density = DEFAULT_BYTES_PER_CELL;
    this.extent = undefined;
    this.generation = undefined;
  }
  plan(bounds: ViewBounds, previous?: ViewBounds) {
    return bufferedViewport(bounds, previous, this.budget, this.density, this.extent);
  }
  get retainedBytes() { return this.bytes; }
  find(bounds: ViewBounds) {
    // Prefer the currently displayed/latest window when several overlap.
    for (let i = this.entries.length - 1; i >= 0; i--) {
      const entry = this.entries[i];
      if (!viewportCovers(entry.view, bounds)) continue;
      this.entries.splice(i,1);
      this.entries.push(entry);
      return entry.view;
    }
    return undefined;
  }
  put(view: ViewportResponse) {
    if (this.generation !== undefined && view.generation !== this.generation) this.clear();
    this.generation = view.generation;
    this.extent = {total_rows:view.total_rows,total_cols:view.total_cols};
    const bytes = retainedBytes(view);
    const cells = (view.loaded_ranges ?? [view]).reduce((sum,r) =>
      sum + (r.end_row-r.start_row+1)*(r.end_col-r.start_col+1), 0);
    this.density = Math.max(DEFAULT_BYTES_PER_CELL, Math.ceil(bytes / Math.max(1,cells)), this.density * 0.8);
    if (bytes > this.budget) {
      // The visible response is still rendered; retain no extra windows beside it.
      this.entries = [];
      this.bytes = 0;
      return;
    }
    while (this.entries.length && (this.bytes + bytes > this.budget || this.entries.length >= MAX_WINDOWS)) {
      this.bytes -= this.entries.shift()!.bytes;
    }
    this.entries.push({view,bytes});
    this.bytes += bytes;
  }
}

export interface GridPoint { row: number; col: number }
export interface GridRange { start: GridPoint; end: GridPoint }
export const MAX_RANGE_CELLS = 10_000;
// Validate before creating matrices or issuing bulk-write requests.
export function boundedRangeDimensions(range: GridRange, limit = MAX_RANGE_CELLS): { rows: number; columns: number; count: number } {
  if (!Number.isSafeInteger(limit) || limit < 1) throw new Error('Invalid range limit');
  for (const coordinate of [range.start.row, range.start.col, range.end.row, range.end.col]) {
    if (!Number.isSafeInteger(coordinate) || coordinate < 0 || coordinate > 0xffffffff) throw new Error('Selection has invalid coordinates');
  }
  const rows = range.end.row - range.start.row + 1;
  const columns = range.end.col - range.start.col + 1;
  if (rows < 1 || columns < 1) throw new Error('Selection bounds must be ordered');
  // Division checks the limit without multiplying arbitrarily large dimensions.
  if (rows > Math.floor(limit / columns)) throw new Error(`Select at most ${limit.toLocaleString('en-US')} cells for this operation`);
  return { rows, columns, count: rows * columns };
}
// Source-table formatting stores rectangular rules, but ordinary cells are materialized.
export function ordinaryFormatRangeIsBounded(range: GridRange, tables: readonly GridRange[]): boolean {
  let uncovered = [range];
  for (const table of tables) {
    uncovered = uncovered.flatMap(block => {
      const top = Math.max(block.start.row, table.start.row);
      const left = Math.max(block.start.col, table.start.col);
      const bottom = Math.min(block.end.row, table.end.row);
      const right = Math.min(block.end.col, table.end.col);
      if (top > bottom || left > right) return [block];
      const pieces: GridRange[] = [];
      const add = (r0: number, c0: number, r1: number, c1: number) => {
        if (r0 <= r1 && c0 <= c1) pieces.push({start:{row:r0,col:c0},end:{row:r1,col:c1}});
      };
      add(block.start.row, block.start.col, top - 1, block.end.col);
      add(bottom + 1, block.start.col, block.end.row, block.end.col);
      add(top, block.start.col, bottom, left - 1);
      add(top, right + 1, bottom, block.end.col);
      return pieces;
    });
  }
  let remaining = MAX_RANGE_CELLS;
  for (const block of uncovered) {
    const rows = block.end.row - block.start.row + 1;
    const columns = block.end.col - block.start.col + 1;
    if (rows > Math.floor(remaining / columns)) return false;
    remaining -= rows * columns;
  }
  return true;
}
export function normalizedRange(a: GridPoint, b: GridPoint): GridRange {
  return { start: { row: Math.min(a.row, b.row), col: Math.min(a.col, b.col) }, end: { row: Math.max(a.row, b.row), col: Math.max(a.col, b.col) } };
}
export function containsCell(range: GridRange, row: number, col: number): boolean {
  return row >= range.start.row && row <= range.end.row && col >= range.start.col && col <= range.end.col;
}
// Spreadsheet clipboard fields use TSV with CSV-style quoting for embedded tabs/newlines.
export function encodeTsv(rows: string[][]): string {
  return rows.map(row => row.map(value => /[\t\r\n"]/.test(value) ? `"${value.replace(/"/g, '""')}"` : value).join('\t')).join('\n');
}
export function decodeTsv(text: string): string[][] {
  if (text.length > 8 * 1024 * 1024) throw new Error('Paste at most 8 MiB of text at a time');
  const rows: string[][] = []; let row: string[] = []; let field = ''; let quoted = false;
  let fieldCount = 0;
  const appendField = () => {
    if (++fieldCount > MAX_RANGE_CELLS) throw new Error('Paste at most 10,000 cells at a time');
    row.push(field); field = '';
  };
  for (let i = 0; i < text.length; i++) {
    const char = text[i];
    if (char === '"' && (quoted || field === '')) {
      if (quoted && text[i + 1] === '"') { field += '"'; i++; } else quoted = !quoted;
    } else if (!quoted && (char === '\t' || char === '\n' || char === '\r')) {
      appendField();
      if (row.length > 10000 || rows.length >= 10000) throw new Error('Paste at most 10,000 cells at a time');
      if (char !== '\t') { rows.push(row); row = []; if (char === '\r' && text[i + 1] === '\n') i++; }
    } else field += char;
  }
  if (quoted) throw new Error('Clipboard contains an unterminated quoted field');
  if (field !== '' || row.length || !rows.length) { appendField(); rows.push(row); }
  const width = rows.reduce((max, r) => Math.max(max, r.length), 0);
  if (rows.length * width > 10000) throw new Error('Paste at most 10,000 cells at a time');
  return rows.map(r => [...r, ...Array(width - r.length).fill('')]);
}

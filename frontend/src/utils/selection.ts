export interface GridPoint { row: number; col: number }
export interface GridRange { start: GridPoint; end: GridPoint }
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
  for (let i = 0; i < text.length; i++) {
    const char = text[i];
    if (char === '"' && (quoted || field === '')) {
      if (quoted && text[i + 1] === '"') { field += '"'; i++; } else quoted = !quoted;
    } else if (!quoted && (char === '\t' || char === '\n' || char === '\r')) {
      row.push(field); field = '';
      if (row.length > 10000 || rows.length >= 10000) throw new Error('Paste at most 10,000 cells at a time');
      if (char !== '\t') { rows.push(row); row = []; if (char === '\r' && text[i + 1] === '\n') i++; }
    } else field += char;
  }
  if (quoted) throw new Error('Clipboard contains an unterminated quoted field');
  if (field !== '' || row.length || !rows.length) { row.push(field); rows.push(row); }
  const width = rows.reduce((max, r) => Math.max(max, r.length), 0);
  if (rows.length * width > 10000) throw new Error('Paste at most 10,000 cells at a time');
  return rows.map(r => [...r, ...Array(width - r.length).fill('')]);
}

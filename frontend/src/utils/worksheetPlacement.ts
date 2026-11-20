import { a1ToCoord } from './coords.ts';

export interface PlacementDraft {
  sheetName: string;
  anchor: string;
}

export interface GridPlacement {
  sheetName: string;
  row: number;
  col: number;
}

export function parsePlacement(draft: PlacementDraft, sheets: readonly string[]): GridPlacement | null {
  const anchor = a1ToCoord(draft.anchor);
  // Backend coordinates are u32, and a sheet must represent a dimension at
  // least one beyond its final coordinate. The server checks the full span.
  if (!sheets.includes(draft.sheetName) || !anchor || anchor.row >= 0xffffffff || anchor.col >= 0xffffffff) return null;
  return { sheetName: draft.sheetName, ...anchor };
}

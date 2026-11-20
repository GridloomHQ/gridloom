export interface NavigationKey {
  key: string;
  shiftKey?: boolean;
  ctrlKey?: boolean;
  metaKey?: boolean;
  altKey?: boolean;
}
export interface GridPosition { row: number; col: number }

/** Pure worksheet navigation. Editing inputs keep their own text navigation. */
export function gridNavigation(
  event: NavigationKey, position: GridPosition, rows: number, cols: number, pageRows: number,
): (GridPosition & { extend: boolean }) | null {
  if (event.altKey) return null;
  const control = event.ctrlKey || event.metaKey;
  let { row, col } = position;
  if (control) {
    if (event.key !== 'Home') return null;
    row = 0; col = 0;
  } else {
    switch (event.key) {
      case 'ArrowUp': row--; break;
      case 'ArrowDown': row++; break;
      case 'ArrowLeft': col--; break;
      case 'ArrowRight': col++; break;
      case 'Enter': row += event.shiftKey ? -1 : 1; break;
      case 'Tab': col += event.shiftKey ? -1 : 1; break;
      case 'Home': col = 0; break;
      case 'PageUp': row -= Math.max(1, pageRows); break;
      case 'PageDown': row += Math.max(1, pageRows); break;
      default: return null;
    }
  }
  return {
    row: Math.max(0, Math.min(Math.max(0, rows - 1), row)),
    col: Math.max(0, Math.min(Math.max(0, cols - 1), col)),
    extend: !!event.shiftKey && event.key !== 'Enter' && event.key !== 'Tab',
  };
}

export type JumpDirection = 'up' | 'down' | 'left' | 'right';
export type SelectionAxis = 'row' | 'column';

export function gridBoundaryShortcut(event: NavigationKey):
  { direction: JumpDirection; extend: boolean } | { axis: SelectionAxis } | null {
  if (event.altKey) return null; // Includes AltGr.
  const control = event.ctrlKey || event.metaKey;
  if (event.key === ' ') {
    if (control && !event.shiftKey) return {axis:'column'};
    if (!control && event.shiftKey) return {axis:'row'};
    return null;
  }
  if (!control) return null;
  switch (event.key) {
    case 'ArrowUp': return {direction:'up',extend:!!event.shiftKey};
    case 'ArrowDown': return {direction:'down',extend:!!event.shiftKey};
    case 'ArrowLeft': return {direction:'left',extend:!!event.shiftKey};
    case 'ArrowRight': return {direction:'right',extend:!!event.shiftKey};
    default: return null;
  }
}

export function wholeAxisRange(axis: SelectionAxis, at: GridPosition, rows: number, columns: number) {
  if (axis === 'row') return {
    start: {row:at.row,col:0}, end:{row:at.row,col:columns-1},
  };
  return {start:{row:0,col:at.col}, end:{row:rows-1,col:at.col}};
}

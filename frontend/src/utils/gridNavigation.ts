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

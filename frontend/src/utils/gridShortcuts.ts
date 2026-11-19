import type { NavigationKey } from './gridNavigation';

export function gridShortcut(key: NavigationKey & { code?: string }) {
  if (!(key.ctrlKey || key.metaKey)) return null;
  const letter = key.key.toLowerCase();
  if (key.altKey) return !key.shiftKey && (letter === 'v' || key.code === 'KeyV') ? 'paste-special' : null;
  if (letter === 'z') return key.shiftKey ? 'redo' : 'undo';
  if (key.shiftKey) return null;
  switch (letter) {
    case 'y': return 'redo';
    case 'd': return 'fill-down';
    case 'r': return 'fill-right';
    default: return null;
  }
}

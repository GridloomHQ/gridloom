import type { CellRenderData } from '../types';
import { displayValue } from './values.ts';
/** Preserve exact entered literals; cached values are only a fallback for source/spill cells. */
export function editableCellInput(cell?: Pick<CellRenderData, 'raw_input' | 'formula' | 'raw_value'>): string {
  return cell?.raw_input ?? cell?.formula ?? (cell ? displayValue(cell.raw_value) : '');
}

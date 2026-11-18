import type {ViewportResponse} from '../types';
export interface LoadedRange {start_row:number;start_col:number;end_row:number;end_col:number;}
export function isCellLoaded(viewport:ViewportResponse|null|undefined,row:number,col:number):boolean {
  return !!viewport && (viewport.loaded_ranges??[viewport]).some(r=>row>=r.start_row&&row<=r.end_row&&col>=r.start_col&&col<=r.end_col);
}

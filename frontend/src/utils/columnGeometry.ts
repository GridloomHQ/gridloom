export const DEFAULT_COLUMN_WIDTH = 110;
export const MIN_COLUMN_WIDTH = 48;
export const MAX_COLUMN_WIDTH = 600;
export function clampColumnWidth(width: number): number {
  return Number.isFinite(width) ? Math.max(MIN_COLUMN_WIDTH, Math.min(MAX_COLUMN_WIDTH, Math.round(width))) : DEFAULT_COLUMN_WIDTH;
}
/** Sparse prefix deltas avoid allocating one entry per worksheet column. */
export function columnGeometry(count: number, widths: Record<number, number>) {
  const columns = Number.isSafeInteger(count) && count > 0 ? count : 0;
  const entries = Object.entries(widths).map(([key, width]) => [Number(key), width])
    .filter(([col, width]) => Number.isSafeInteger(col) && col >= 0 && col < columns && Number.isInteger(width) && width >= MIN_COLUMN_WIDTH && width <= MAX_COLUMN_WIDTH)
    .sort((a,b) => a[0]-b[0]);
  const keys: number[] = [], prefix = [0], custom = new Map<number, number>();
  for (const [col, raw] of entries) {
    const width = clampColumnWidth(raw);
    // Object keys such as '01' can alias column 1. Keep only one normalized entry.
    if(custom.has(col)) continue;
    keys.push(col);custom.set(col,width);prefix.push(prefix[prefix.length-1]+width-DEFAULT_COLUMN_WIDTH);
  }
  const width = (col:number) => custom.get(col) ?? DEFAULT_COLUMN_WIDTH;
  const offset = (col:number) => {
    col=Number.isFinite(col)?Math.max(0,Math.min(columns,Math.floor(col))):0;
    let lo=0,hi=keys.length;
    while(lo<hi){const mid=Math.floor((lo+hi)/2);if(keys[mid]<col)lo=mid+1;else hi=mid;}
    return col*DEFAULT_COLUMN_WIDTH+prefix[lo];
  };
  const atPixel = (pixel:number) => {
    if(!columns || Number.isNaN(pixel) || pixel<=0)return 0;
    let lo=0,hi=columns;
    while(lo<hi){const mid=Math.floor((lo+hi)/2);if(offset(mid+1)<=pixel)lo=mid+1;else hi=mid;}
    return Math.min(columns-1,lo);
  };
  return {width,offset,atPixel,totalWidth:offset(columns)};
}

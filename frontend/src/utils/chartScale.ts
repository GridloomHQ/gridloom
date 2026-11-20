/** A signed axis including zero, with a non-degenerate domain for zero-only data. */
export function signedScale(series: readonly (readonly number[])[]) {
  let min = 0;
  let max = 0;
  for (const values of series) {
    for (const v of values) {
      if (Number.isFinite(v)) {
        min = Math.min(min, v);
        max = Math.max(max, v);
      }
    }
  }
  if (min === max) max = 1;
  const span = max - min;
  const fraction = (v: number) => (v - min) / span;
  return { min, max, fraction };
}

export function barSpan(value: number, project: (v: number) => number) {
  const zero = project(0);
  const end = project(value);
  return { start: Math.min(zero, end), length: Math.abs(end - zero) };
}

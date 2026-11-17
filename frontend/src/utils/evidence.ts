export const MAX_ATTACHMENT_BYTES = 2 * 1024 * 1024;
export const MAX_REVIEW_BYTES = 8 * 1024 * 1024;
export function validateAttachmentSizes(existing: readonly number[], incoming: readonly number[]): string | null {
  const sizes = [...existing, ...incoming];
  if (sizes.length > 20) return 'Attach at most 20 files.';
  if (sizes.some(n => !Number.isSafeInteger(n) || n < 0 || n > MAX_ATTACHMENT_BYTES)) return 'Each attachment must be at most 2 MiB.';
  if (sizes.reduce((n, size) => n + size, 0) > MAX_REVIEW_BYTES) return 'Attached evidence must total at most 8 MiB per review.';
  return null;
}
/** Download names are presentation only, never paths or HTML. */
export function evidenceDownloadName(filename: string): string {
  const clean = filename.split(/[\\/]/).pop()?.replace(/[\u0000-\u001f\u007f]/g, '').trim();
  return clean && clean !== '.' && clean !== '..' ? clean.slice(0, 200) : 'evidence.bin';
}

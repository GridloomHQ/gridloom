import { reviewStatus } from './reviews.ts';
import type { CellValue, ReviewPack, CloseQueueItem } from '../types';

const DECIMAL = /^-?\d+(?:\.\d+)?$/;
export function exactPlAmount(value: CellValue): { text: string; numeric: boolean; state: string } {
  if (value.type === 'Decimal' && DECIMAL.test(value.value)) return { text: value.value, numeric: true, state: 'Exact decimal' };
  if (value.type === 'Integer' && Number.isSafeInteger(value.value)) return { text: String(value.value), numeric: true, state: 'Exact integer' };
  if (value.type === 'Error') return { text: `Error: ${JSON.stringify(value.value)}`, numeric: false, state: 'Calculation error' };
  if (value.type === 'Null' || value.type === 'Blank' || value.type === 'EmptyText') return { text: `Missing amount (${value.type})`, numeric: false, state: 'Missing amount' };
  return { text: `Invalid amount (${value.type})`, numeric: false, state: 'Not an exact numeric amount' };
}
/** Never coerce decimals through Number. Only typed, validated amount fields bypass text protection. */
export function csvField(value: string, numeric = false): string {
  if (numeric && !DECIMAL.test(value)) throw new Error('CSV amount must be a validated decimal');
  const protectedValue = !numeric && /^[\s\u0000-\u001f]*[=+\-@]/.test(value) ? `'${value}` : value;
  // A tab/CR/LF at the start is itself interpreted specially by some spreadsheet importers.
  const safe = !numeric && /^[\t\r\n]/.test(protectedValue) ? `'${protectedValue}` : protectedValue;
  return `"${safe.replace(/"/g, '""')}"`;
}
/** Import version alone does not identify worksheet edits made after import. */
function currentLineSnapshot(pack: ReviewPack, item: CloseQueueItem): string {
  const explanation = pack.explanations.find(e => e.sheet_name === pack.status.period.sheet_name && e.coord.row === item.coord.row && e.coord.col === item.coord.col);
  return explanation?.snapshot || 'Unavailable (no matching captured explanation)';
}
export function exportPlCsv(pack: ReviewPack): string {
  const rows = [['Period', 'Import version', 'Report line', 'Amount', 'Value state', 'Review status', 'Exported at', 'Period ID', 'Worksheet', 'Current calculation snapshot', 'Entity', 'Currency', 'Units'].map(v => csvField(v)).join(',')];
  for (const item of pack.status.queue) {
    const amount = exactPlAmount(item.value);
    rows.push([csvField(pack.status.period.period), csvField(String(pack.status.period.version)), csvField(item.label), csvField(amount.text, amount.numeric), csvField(amount.state), csvField(item.status), csvField(pack.exported_at), csvField(pack.status.period.id), csvField(pack.status.period.sheet_name), csvField(currentLineSnapshot(pack, item)), csvField(pack.status.period.entity ?? ''), csvField(pack.status.period.currency ?? ''), csvField(pack.status.period.units ?? '')].join(','));
  }
  return `${rows.join('\r\n')}\r\n`;
}
export function escapeHtml(value: unknown): string {
  return String(value ?? '').replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]!));
}
const cell = (value: unknown) => `<td>${escapeHtml(value)}</td>`;
function table(headers: string[], rows: string[]): string {
  return `<div class="table-wrap"><table><thead><tr>${headers.map(h => `<th>${escapeHtml(h)}</th>`).join('')}</tr></thead><tbody>${rows.join('')}</tbody></table></div>`;
}
const amountText = (value: string | null | undefined): string => value != null && DECIMAL.test(value) ? value : 'Invalid amount';
export function exportCloseHtml(pack: ReviewPack): string {
  const { status } = pack, period = status.period;
  const pl = table(['Report line', 'Amount', 'Value state', 'Review status', 'Current calculation snapshot'], status.queue.map(item => {
    const amount = exactPlAmount(item.value);
    return `<tr>${cell(item.label)}${cell(amount.text)}${cell(amount.state)}${cell(item.status)}${cell(currentLineSnapshot(pack, item))}</tr>`;
  }));
  const accounts = table(['Worksheet row', 'Imported account', 'Current account', 'Name', 'Mapping', 'Worksheet debit', 'Worksheet credit', 'Imported net', 'Adjustment net', 'Reported net', 'Identity check'], status.live_accounts.map(a =>
    `<tr>${cell(a.row + 1)}${cell(a.imported_account)}${cell(a.account)}${cell(a.name)}${cell(a.mapping || 'Unmapped')}${cell(amountText(a.debit))}${cell(amountText(a.credit))}${cell(amountText(a.imported_balance))}${cell(amountText(a.adjustment_balance))}${cell(amountText(a.reported_balance))}${cell(a.identity_matches_import ? 'Matches import' : 'Changed from import')}</tr>`));
  const reviews = pack.reviews.map(review => `<article><h3>${escapeHtml(review.sheet_name)} · row ${review.coord.row + 1}, column ${review.coord.col + 1}</h3>
<p><strong>${escapeHtml(reviewStatus(review))}</strong> · ${escapeHtml(review.reviewer)} (self-reported) · ${escapeHtml(review.created_at)}</p>
${review.invalidation_reason ? `<p>${escapeHtml(review.invalidation_reason)}</p>` : ''}<p class="note">${escapeHtml(review.note || '(No note)')}</p>
<p class="hash">Review ID: ${escapeHtml(review.id)}<br>Snapshot: ${escapeHtml(review.snapshot)}</p>
${table(['Evidence filename', 'Version', 'SHA-256', 'Retained bytes', 'Previous version ID'], (review.attachments ?? []).map(a => `<tr>${cell(a.filename)}${cell(a.version)}${cell(a.sha256)}${cell(a.bytes.length)}${cell(a.previous_version_id ?? 'New evidence item')}</tr>`))}
${(review.question_events ?? []).map(event => `<p><strong>${escapeHtml(event.action)}</strong> · ${escapeHtml(event.actor)} · ${escapeHtml(event.created_at)}</p><p class="note">${escapeHtml(event.note)}</p><p class="hash">Snapshot: ${escapeHtml(event.snapshot)}</p>`).join('')}
${review.evidence.length ? `<h4>Plain-text evidence references (not fetched or verified)</h4>${table(['Label', 'Reference'], review.evidence.map(e => `<tr>${cell(e.label)}${cell(e.reference)}</tr>`))}` : ''}</article>`).join('');
  const journals = (period.adjustments ?? []).map(entry => `<article><h3>${escapeHtml(entry.author)} · ${escapeHtml(entry.created_at)}</h3><p class="note">${escapeHtml(entry.rationale)}</p>
<p class="hash">Adjustment ID: ${escapeHtml(entry.id)}</p>
${table(['Account', 'Debit', 'Credit'], entry.lines.map(line => `<tr>${cell(line.account)}${cell(amountText(line.debit))}${cell(amountText(line.credit))}</tr>`))}
${table(['Evidence filename', 'SHA-256', 'Retained bytes'], entry.evidence.map(file => `<tr>${cell(file.filename)}${cell(file.sha256)}${cell(file.bytes.length)}</tr>`))}</article>`).join('');
  const hierarchy = (period.report_lines ?? []).length ? '<h2>Report hierarchy</h2>' + table(['Line', 'ID', 'Parent ID', 'Normal balance'], period.report_lines!.map(line => `<tr>${cell(line.label)}${cell(line.id)}${cell(line.parent_id ?? 'Top level')}${cell(line.normal_balance)}</tr>`)) : '';
  const warnings = [...status.warnings, ...pack.limitations];
  return `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'none'; style-src 'unsafe-inline'; img-src 'none'; connect-src 'none'; font-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Tayriva close summary · ${escapeHtml(period.period)}</title>
<style>body{font:15px/1.5 system-ui,sans-serif;color:#172033;background:#f4f6fa;margin:0}main{max-width:1160px;margin:32px auto;padding:32px;background:white}h1,h2,h3{line-height:1.2}h1{margin-bottom:8px}h2{margin-top:32px;border-bottom:2px solid #dbe3ef;padding-bottom:8px}.notice{padding:16px;background:#fff4d6;border-left:4px solid #ba8200}.meta{color:#46546b}.hash{font:12px/1.6 monospace;overflow-wrap:anywhere}.table-wrap{overflow:auto}table{border-collapse:collapse;width:100%;margin:12px 0}th,td{border:1px solid #dbe3ef;text-align:left;vertical-align:top;padding:8px;overflow-wrap:anywhere}th{background:#eef2f8}td{font-variant-numeric:tabular-nums}article{border:1px solid #dbe3ef;padding:16px;margin:16px 0;break-inside:avoid}.note{white-space:pre-wrap;overflow-wrap:anywhere}li{overflow-wrap:anywhere}@media print{body{background:white}main{margin:0;padding:0;max-width:none}table{font-size:10px}h2{break-after:avoid}.table-wrap{overflow:visible}th{print-color-adjust:exact}}</style></head><body><main>
<h1>Monthly close summary</h1><p class="meta">Period ${escapeHtml(period.period)} · import version ${escapeHtml(period.version)} · worksheet ${escapeHtml(period.sheet_name)}</p>
<p class="meta">Entity: ${escapeHtml(period.entity || "(not specified)")} · Currency: ${escapeHtml(period.currency || "(not specified)")} · Units: ${escapeHtml(period.units || "(not specified)")}</p>
<p class="meta">Exported ${escapeHtml(pack.exported_at)} · Tayriva review-pack format ${escapeHtml(pack.version)}</p><p class="hash">Source CSV SHA-256: ${escapeHtml(period.source_sha256)}<br>Period ID: ${escapeHtml(period.id)}</p>
<div class="notice"><strong>Local review notebook, not audit assurance.</strong> Reviewer identities are self-reported and unauthenticated. This summary does not prove ledger completeness or accuracy. Evidence contents, full captured inputs, original CSV and worksheet details are omitted here: retain the companion JSON review pack for those records. Support files and references are not opened, fetched or executed. Report CSV retains exact decimal text; spreadsheet importers may round numeric cells, so use text import for exact preservation. Currency and units apply to this working paper only; no conversion is performed. Import version identifies the imported trial balance, not later worksheet edits. Match current calculation snapshot IDs with the JSON pack before treating separately downloaded exports as the same calculation state; a snapshot ID is not an authenticated signature.</div>
<h2>Reconciliation and close readiness</h2>${table(['Debits', 'Credits', 'Difference', 'Balance check', 'Close readiness'], [`<tr>${cell(amountText(status.debits))}${cell(amountText(status.credits))}${cell(amountText(status.difference))}${cell(status.balanced ? 'Balanced' : 'Not balanced / invalid amounts')}${cell(status.ready_for_close ? 'All current close checks satisfied' : 'Not ready for close')}</tr>`])}
<p>Unmapped accounts: ${escapeHtml(status.unmapped_accounts.join(', ') || 'None')}</p>
${warnings.length ? `<ul>${warnings.map(w => `<li>${escapeHtml(w)}</li>`).join('')}</ul>` : ''}
${hierarchy}
<h2>${period.report_lines?.length ? 'Report' : 'Profit and loss'}</h2>${pl}<h2>Account mappings and balance reconciliation</h2>
<p>Debit-positive net amounts reconcile imported net plus documented adjustments to reported net. The retained original CSV is in the JSON pack.</p>${accounts}
<h2>Posted adjustments and evidence inventory</h2>${journals || '<p>No adjustment entries were posted.</p>'}
<h2>Review history and evidence inventory</h2>${reviews || '<p>No reviews recorded for this period.</p>'}
</main></body></html>`;
}

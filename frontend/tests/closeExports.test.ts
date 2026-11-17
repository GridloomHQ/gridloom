import test from 'node:test';
import assert from 'node:assert/strict';
import { csvField, escapeHtml, exactPlAmount, exportPlCsv, exportCloseHtml } from '../src/utils/closeExports.ts';
import type { ReviewPack } from '../src/types';
const attack = '</td><script src="https://evil.invalid/x">alert(1)</script><img src=x onerror=alert(1)>';
function fixture(): ReviewPack {
  return {
    format: 'gridloom-review-pack', version: 1, exported_at: '2026-11-18T12:00:00Z', worksheet: {}, explanations: [], limitations: ['Unauthenticated local reviews'],
    status: {
      period: { id: 'period-1', period: '2026-10', version: 2, sheet_name: 'October', created_at: '2026-11-01', source_csv: 'private raw source', source_sha256: 'source-hash', accounts: [], prior_period_id: null },
      live_accounts: [{ row: 1, imported_account: '4000', account: '4000', name: attack, mapping: 'Revenue', debit: null, credit: '12345678901234567890.12345678', identity_matches_import: true }],
      debits: '100.00', credits: '100.00', difference: '0.00', balanced: true, ready_for_close: false, unmapped_accounts: [], warnings: [attack],
      queue: [{ label: '=WEBSERVICE("https://evil.invalid")', coord: {row: 1, col: 8}, value: { type: 'Decimal', value: '-12345678901234567890.12345678' }, status: 'Needs re-review', review_id: 'review-1' }],
    },
    reviews: [{ id: 'review-1', sheet_name: 'October', coord: { row: 1, col: 8 }, decision: 'Reviewed', reviewer: attack, note: attack, evidence: [{label: 'external', reference: 'javascript:alert(1)'}], created_at: '2026-11-01', snapshot: 'v1:abc', invalidated: true, invalidation_reason: 'Account changed', attachments: [{id: 'attachment-1', previous_version_id: 'attachment-0', filename: attack, media_type: 'text/html', sha256: 'evidence-hash', version: 2, bytes: [60, 62, 120], created_at: '2026-11-01'}] }],
  };
}
test('CSV preserves exact decimal characters, including negative amounts, and protects text', () => {
  const csv = exportPlCsv(fixture());
  assert.ok(csv.includes('"-12345678901234567890.12345678"'));
  assert.ok(!csv.includes('"\'-12345678901234567890.12345678"'));
  assert.ok(csv.includes('"\'=WEBSERVICE(""https://evil.invalid"")"'));
  assert.ok(csv.startsWith('"Period","Import version","P&L line","Amount","Value state","Review status","Exported at","Period ID","Worksheet","Current calculation snapshot"\r\n'));
  assert.ok(csv.endsWith('\r\n'));
  assert.equal(csvField('text, "quoted"\nnext'), '"text, ""quoted""\nnext"');
  for (const value of ['=1+2', '+1+2', '-1+2', '@SUM(A1)', '  =1+2', '\t=1', '\ntext', '\rtext']) assert.ok(csvField(value).startsWith('"\''));
  assert.throws(() => csvField('=1+2', true));
});
test('invalid, missing, approximate and unsafe numeric P&L values remain explicit', () => {
  assert.deepEqual(exactPlAmount({type: 'Decimal', value: '1.00000000000000000001'}), {text:'1.00000000000000000001',numeric:true,state:'Exact decimal'});
  for (const value of [{type:'Float',value:1.5}, {type:'Integer',value:9007199254740992}, {type:'Decimal',value:'1e3'}, {type:'Text',value:'=1+2'}] as const) assert.equal(exactPlAmount(value).numeric, false);
  for (const type of ['Null', 'Blank', 'EmptyText'] as const) assert.match(exactPlAmount({type}).text, /Missing amount/);
  assert.match(exactPlAmount({type:'Error',value:'Div0'}).text, /Error.*Div0/);
  const pack=fixture(); pack.status.queue[0].value={type:'Error',value:'Div0'};
  assert.match(exportPlCsv(pack), /Calculation error/);
  assert.match(exportCloseHtml(pack), /Error: &quot;Div0&quot;/);
});
test('HTML is escaped passive content with restrictive CSP, no active support or network resources', () => {
  const html = exportCloseHtml(fixture());
  assert.ok(html.startsWith('<!doctype html>'));
  assert.ok(html.includes('default-src \'none\''));
  assert.ok(html.includes('script-src \'none\''));
  assert.ok(html.includes('connect-src \'none\''));
  assert.ok(html.includes('base-uri \'none\''));
  assert.ok(!/<script|<img|<iframe|<object|<a\b|<form\b|<[^>]+\s(?:href|src|onerror)=/i.test(html));
  assert.ok(html.includes(escapeHtml(attack)));
  for (const text of ['source-hash','evidence-hash','v1:abc','attachment-0','Needs re-review','Invalid amount','self-reported','unauthenticated','companion JSON','not audit assurance','Current account mappings']) assert.ok(html.includes(text), text);
  assert.ok(!html.includes('private raw source'));
  assert.ok(!html.includes('[60,62,120]'));
});

test('exports identify current calculation state, not only the unchanged import version', () => {
  const pack = fixture();
  const item = pack.status.queue[0];
  pack.explanations = [{sheet_name: pack.status.period.sheet_name, coord: item.coord, value: item.value,
    formula: '=SUM(A1:A2)', nodes: [], warnings: [], snapshot: 'v1:current', can_review: true, review: null}];
  const csv = exportPlCsv(pack), html = exportCloseHtml(pack);
  for (const value of [pack.exported_at, pack.status.period.id, pack.status.period.sheet_name, 'v1:current']) {
    assert.ok(csv.includes(value), value); assert.ok(html.includes(value), value);
  }
  assert.ok(html.includes('Import version identifies the imported trial balance, not later worksheet edits'));
  assert.ok(html.includes('snapshot ID is not an authenticated signature'));
  pack.explanations[0].sheet_name = 'Different period';
  assert.ok(exportPlCsv(pack).includes('Unavailable (no matching captured explanation)'));
  assert.ok(exportCloseHtml(pack).includes('Unavailable (no matching captured explanation)'));
});

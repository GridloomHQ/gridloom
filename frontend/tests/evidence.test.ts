import test from 'node:test';
import assert from 'node:assert/strict';
import { MAX_ATTACHMENT_BYTES as M, validateAttachmentSizes } from '../src/utils/evidence.ts';
test('evidence validates each file, existing draft bytes and count before reading', () => {
  assert.equal(validateAttachmentSizes([M], [M, M, M]), null);
  assert.match(validateAttachmentSizes([], [M + 1])!, /2 MiB/);
  assert.match(validateAttachmentSizes([M, M, M, M], [1])!, /8 MiB/);
  assert.match(validateAttachmentSizes(Array(20).fill(1), [1])!, /20 files/);
  assert.match(validateAttachmentSizes([], [-1])!, /2 MiB/);
  assert.match(validateAttachmentSizes([], [NaN])!, /2 MiB/);
});
import { evidenceDownloadName } from '../src/utils/evidence.ts';
test('attachment downloads use plain leaf filenames without control characters', () => {
  assert.equal(evidenceDownloadName('../../support.pdf'), 'support.pdf');
  assert.equal(evidenceDownloadName('C:\\docs\\support.pdf'), 'support.pdf');
  assert.equal(evidenceDownloadName('\u0000'), 'evidence.bin');
  assert.equal(evidenceDownloadName('..'), 'evidence.bin');
});

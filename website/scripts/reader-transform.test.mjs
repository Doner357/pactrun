import assert from 'node:assert/strict';
import test from 'node:test';
import {references, renderReference} from './reader-references.mjs';
const entry = references.find(item => item.audience === 'Authors' && item.source.endsWith('/hook-protocol.md'));

test('reader projection preserves fenced payloads and inline code literally', () => {
  const literal = '```text\n### PR-REQ-9001 - Literal example\n**Verification: literal payload text.**\n[example](./pack-source.md#literal)\n```';
  const inline = '`[example](./pack-source.md#literal)`';
  const source = '# Hook owner\n\n**Status: Normative.**\n\n### PR-REQ-9000 - Real section\n\n' + literal + '\n\nUse ' + inline + ' as data.\n';
  const rendered = renderReference(entry, source);
  assert.ok(rendered.includes(literal), 'Fenced data was changed by prose rewriting');
  assert.ok(rendered.includes(inline), 'Inline code was changed by link rewriting');
  assert.ok(rendered.includes('### Real section {#pr-req-9000---real-section}'));
  assert.ok(!rendered.includes('**Status: Normative.**'));
});

test('tilde and long backtick fences retain nested-looking code', () => {
  for (const fence of ['~~~~', '````']) {
    const literal = fence + '\n```json\n{"message":"[source](./pack-source.md#field)"}\n```\n' + fence;
    assert.ok(renderReference(entry, '# Owner\n\n' + literal + '\n').includes(literal));
  }
});

test('maintainer source links retain distinct referenced sections', () => {
  const rendered = renderReference(entry, '# Owner\n\nSee [first](../foundations/identity-and-state.md#first) and [second](../foundations/identity-and-state.md#second).\n');
  assert.ok(rendered.includes('[first](../../spec/foundations/identity-and-state.md#first)'));
  assert.ok(rendered.includes('[second](../../spec/foundations/identity-and-state.md#second)'));
});

test('ordinary bare relative document links also route to the same-role view', () => {
  const rendered = renderReference(entry, '# Owner\n\nSee [source](pack-source.md#field).\n');
  assert.ok(rendered.includes('[source](./source-format.md#field)'));
});

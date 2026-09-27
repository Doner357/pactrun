// Test-ID: PR-TEST-0492
// Verifies: PR-REQ-0348
// Independent canonical components, complete frame and digest. Inputs are
// normalized fixtures, not Rust-produced bytes; no raw-validation claim.
import {createHash} from 'node:crypto';
import {readFileSync} from 'node:fs';
const fixture = JSON.parse(readFileSync(new URL('../vectors/revision_canonical/shell.json', import.meta.url), 'utf8'));
const results = [];
for (const entry of fixture.cases) {
  const core = structuredClone(fixture.normalized_core);
  core.actions[0].hook.launch.shell = entry.shell;
  const a = Buffer.from(jcs(core)), b = Buffer.from(jcs(fixture.normalized_content));
  const length = bytes => { const out = Buffer.alloc(8); out.writeBigUInt64BE(BigInt(bytes.length)); return out; };
  const frame = Buffer.concat([Buffer.from('pactrun.revision-content-digest\0'),
    Buffer.from('revision-core\0'), length(a), a, Buffer.from('runtime-content-closure\0'), length(b), b]);
  const digest = 'sha256:' + createHash('sha256').update(frame).digest('hex');
  if (!process.argv.includes('--calculate') && digest !== entry.digest) throw new Error('Core V3 independent digest mismatch: ' + entry.shell);
  results.push({shell:entry.shell, core_jcs_hex:a.toString('hex'), content_jcs_hex:b.toString('hex'), frame_hex:frame.toString('hex'), digest});
}
process.stdout.write(JSON.stringify(results));
function jcs(v) {
  if (v === null || ['string','boolean'].includes(typeof v)) return JSON.stringify(v);
  if (typeof v === 'number') { if (!Number.isFinite(v)) throw new Error('non-finite number'); return JSON.stringify(v); }
  if (Array.isArray(v)) return '[' + v.map(jcs).join(',') + ']';
  if (typeof v === 'object') return '{' + Object.keys(v).sort().map(k=>JSON.stringify(k)+':'+jcs(v[k])).join(',') + '}';
  throw new Error('invalid normalized JSON');
}

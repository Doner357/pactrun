// Test-ID: PR-TEST-0332
// Verifies: PR-REQ-0318
// Independent byte/digest oracle over checked-in normalized inputs. It does
// not consume Rust-produced bytes or claim raw duplicate-key validation.
import {createHash} from 'node:crypto';
import {readFileSync} from 'node:fs';

if (Number.parseInt(process.versions.node.split('.')[0], 10) !== 24) {
  throw new Error('Core V2 oracle requires Node 24');
}
const manifest = JSON.parse(readFileSync(process.argv[2], 'utf8'));
const calculated = {};
for (const vector of manifest.valid) {
  const core = Buffer.from(jcs(vector.normalized_core), 'utf8');
  const content = Buffer.from(jcs(vector.normalized_content), 'utf8');
  const version = Buffer.alloc(4); version.writeUInt32BE(2);
  const length = value => { const b = Buffer.alloc(8); b.writeBigUInt64BE(BigInt(value)); return b; };
  const frame = Buffer.concat([
    Buffer.from('pactrun.revision-content-digest\0', 'ascii'), version,
    Buffer.from('revision-core\0', 'ascii'), length(core.length), core,
    Buffer.from('runtime-content-closure\0', 'ascii'), length(content.length), content,
  ]);
  const actual = {core_jcs_hex: core.toString('hex'), content_jcs_hex: content.toString('hex'),
    frame_hex: frame.toString('hex'), digest: 'sha256:' + createHash('sha256').update(frame).digest('hex')};
  calculated[vector.name] = actual;
  if (!process.argv.includes('--calculate') && JSON.stringify(actual) !== JSON.stringify(vector.expected)) {
    throw new Error(vector.name + ': independent Core V2 oracle mismatch');
  }
}
if (process.argv.includes('--calculate')) process.stdout.write(JSON.stringify(calculated, null, 2) + '\n');
else process.stderr.write('Core V2 Node oracle: ' + manifest.valid.length + ' vectors passed\n');

function jcs(value) {
  if (value === null || typeof value === 'boolean' || typeof value === 'string') return JSON.stringify(value);
  if (typeof value === 'number') {
    if (!Number.isFinite(value)) throw new Error('non-finite normalized JCS number');
    return JSON.stringify(Object.is(value, -0) ? 0 : value);
  }
  if (Array.isArray(value)) return '[' + value.map(jcs).join(',') + ']';
  if (typeof value === 'object') return '{' + Object.keys(value).sort().map(k => JSON.stringify(k) + ':' + jcs(value[k])).join(',') + '}';
  throw new Error('unsupported normalized JCS value');
}

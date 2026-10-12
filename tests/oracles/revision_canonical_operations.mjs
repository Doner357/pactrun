// Test-ID: PR-TEST-0701
// Verifies: PR-REQ-0380
// Independent normalized-byte oracle, not a raw JSON/YAML/schema validator.
import {createHash} from 'node:crypto';
import {readFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import path from 'node:path';

function jcs(value) {
  if (value === null || typeof value === 'string' || typeof value === 'boolean') return JSON.stringify(value);
  if (typeof value === 'number' && Number.isFinite(value)) return JSON.stringify(Object.is(value, -0) ? 0 : value);
  if (Array.isArray(value)) return '[' + value.map(jcs).join(',') + ']';
  if (typeof value === 'object') return '{' + Object.keys(value).sort().map(k => JSON.stringify(k) + ':' + jcs(value[k])).join(',') + '}';
  throw new Error('invalid normalized value');
}
export function calculate(core, content) {
  if (core.format_version !== '1.0-alpha.2') throw new Error('wrong operation Core marker');
  const a = Buffer.from(jcs(core)), b = Buffer.from(jcs(content));
  const length = bytes => { const out = Buffer.alloc(8); out.writeBigUInt64BE(BigInt(bytes.length)); return out; };
  const frame = Buffer.concat([
    Buffer.from('pactrun.revision-content-digest\0'), Buffer.from('revision-core\0'), length(a), a,
    Buffer.from('runtime-content-closure\0'), length(b), b,
  ]);
  return {core_jcs_hex: a.toString('hex'), content_jcs_hex: b.toString('hex'), frame_hex: frame.toString('hex'),
    digest: 'sha256:' + createHash('sha256').update(frame).digest('hex')};
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (Number(process.versions.node.split('.')[0]) !== 24) throw new Error('requires Node 24');
  const vectors = JSON.parse(readFileSync(process.argv[2], 'utf8'));
  for (const vector of vectors.valid) {
    if (JSON.stringify(calculate(vector.normalized_core, vector.normalized_content)) !== JSON.stringify(vector.expected)) {
      throw new Error(vector.name + ': operation Core oracle mismatch');
    }
  }
  console.log(`Operation Core Node oracle: ${vectors.valid.length} vectors passed`);
}

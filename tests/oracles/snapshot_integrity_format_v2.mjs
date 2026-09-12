// Test-ID: PR-TEST-0190
// Verifies: PR-REQ-0295, PR-REQ-0296, PR-REQ-0297
// Independent normalized-valid-fixture oracle. It does not validate raw JSON duplicates.
import {createHash} from 'node:crypto';
import {readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';

export function canonicalize(value) {
  if (value === null || ['boolean', 'string'].includes(typeof value)) {
    return JSON.stringify(value);
  }
  if (typeof value === 'number') {
    if (!Number.isFinite(value)) throw new Error('non-finite normalized number');
    return JSON.stringify(Object.is(value, -0) ? 0 : value);
  }
  if (Array.isArray(value)) return `[${value.map(canonicalize).join(',')}]`;
  if (typeof value === 'object') {
    return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonicalize(value[key])}`).join(',')}}`;
  }
  throw new Error('unsupported normalized value');
}

function verifyProducer(vector) {
  const manifest = vector.normalized_manifest;
  const context = vector.producer_context;
  if (!context) {
    if (vector.relational.status !== 'not_evaluated') throw new Error('missing producer must be not_evaluated');
    return;
  }
  if (manifest.producer.package_id !== context.package_id ||
      manifest.producer.revision_content_digest !== context.revision_content_digest) {
    throw new Error('producer identity mismatch');
  }
  const declarations = new Map(context.inputs.map((input) => [input.id, input.protection]));
  const bindings = new Map(manifest.managed_bindings.map((binding) => [binding.input_id, binding]));
  if (declarations.size !== context.inputs.length || bindings.size !== manifest.managed_bindings.length) {
    throw new Error('duplicate producer or binding identity');
  }
  for (const [id, protection] of declarations) {
    const binding = bindings.get(id);
    if (!binding || binding.role !== 'active') throw new Error('missing active producer binding');
    const stickyBound = binding.state === 'bound' && protection === 'normal' && binding.protection === 'secret';
    if (binding.protection !== protection && !stickyBound) throw new Error('invalid V2 protection');
  }
  for (const binding of bindings.values()) {
    if ((binding.role === 'active') !== declarations.has(binding.input_id)) throw new Error('invalid binding role');
    if (binding.role === 'retained' && binding.state !== 'bound') throw new Error('retained absence');
  }
  if (vector.relational.status !== 'valid') throw new Error('evaluated producer must be valid');
}

export function calculate(vector) {
  const manifest = vector.normalized_manifest;
  if (manifest.format_version !== 2) throw new Error('V2 oracle does not upgrade other formats');
  verifyProducer(vector);
  const referenced = new Set([
    ...manifest.managed_bindings.filter((binding) => binding.state === 'bound').map((binding) => binding.blob_digest),
    ...manifest.service_content.map((content) => content.blob_digest),
  ]);
  const blobs = vector.blob_contents ?? {};
  for (const digest of referenced) {
    if (!Object.hasOwn(blobs, digest)) throw new Error('missing referenced public blob');
  }
  for (const [digest, bytesHex] of Object.entries(blobs)) {
    if (!referenced.has(digest) || !/^[0-9a-f]*$/.test(bytesHex) || bytesHex.length % 2 !== 0) {
      throw new Error('invalid public blob closure');
    }
    const actual = `sha256:${createHash('sha256').update(Buffer.from(bytesHex, 'hex')).digest('hex')}`;
    if (actual !== digest) throw new Error('public blob digest mismatch');
  }
  const manifestJcs = Buffer.from(canonicalize(manifest), 'utf8');
  const version = Buffer.alloc(4);
  version.writeUInt32BE(2);
  const length = Buffer.alloc(8);
  length.writeBigUInt64BE(BigInt(manifestJcs.length));
  const frame = Buffer.concat([
    Buffer.from('pactrun.snapshot-integrity-digest\0', 'ascii'), version,
    Buffer.from('snapshot-integrity-manifest\0', 'ascii'), length, manifestJcs,
  ]);
  return {
    manifest_jcs_hex: manifestJcs.toString('hex'),
    frame_hex: frame.toString('hex'),
    digest: `sha256:${createHash('sha256').update(frame).digest('hex')}`,
  };
}

function main() {
  if (Number.parseInt(process.versions.node.split('.')[0], 10) !== 24) {
    throw new Error('Snapshot V2 oracle requires Node 24');
  }
  const path = process.argv[2];
  if (!path) throw new Error('usage: node snapshot_integrity_format_v2.mjs <vectors.json>');
  const corpus = JSON.parse(readFileSync(path, 'utf8'));
  if (corpus.format !== 'snapshot_integrity_format_v2_golden_vectors' ||
      !['candidate', 'frozen'].includes(corpus.status)) throw new Error('invalid vector metadata');
  const names = new Set();
  for (const vector of [...corpus.valid, ...corpus.invalid]) {
    if (names.has(vector.name)) throw new Error('duplicate vector name');
    names.add(vector.name);
  }
  for (const sample of corpus.rfc8785) {
    if (canonicalize(sample.input) !== sample.expected) throw new Error(`${sample.name}: JCS mismatch`);
  }
  for (const vector of corpus.valid) {
    const actual = calculate(vector);
    for (const key of ['manifest_jcs_hex', 'frame_hex', 'digest']) {
      if (actual[key] !== vector.expected[key]) throw new Error(`${vector.name}: ${key} mismatch`);
    }
  }
  process.stdout.write(`SnapshotIntegrityFormatV2 Node 24 oracle: ${corpus.valid.length} valid and ${corpus.rfc8785.length} RFC 8785 vectors passed\n`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) main();

// Test-ID: PR-TEST-0012
// Verifies: PR-REQ-0182, PR-REQ-0183, PR-REQ-0193
import {createHash} from 'node:crypto';
import {readFileSync} from 'node:fs';

const manifestPath = process.argv[2];
if (!manifestPath) {
  throw new Error('usage: node revision_core_format_v1.mjs <vectors.json> [--calculate]');
}

if (Number.parseInt(process.versions.node.split('.')[0], 10) !== 24) {
  throw new Error(`RevisionCoreFormatV1 oracle requires Node 24, found ${process.version}`);
}

const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
const calculateOnly = process.argv.includes('--calculate');
const calculated = {};

for (const vector of manifest.rfc8785) {
  const actual = canonicalize(vector.input);
  if (actual !== vector.expected) {
    throw new Error(`${vector.name}: RFC 8785 sample mismatch`);
  }
}

for (const vector of manifest.valid) {
  const coreJcs = Buffer.from(canonicalize(vector.normalized_core), 'utf8');
  const contentJcs = Buffer.from(canonicalize(vector.normalized_content), 'utf8');
  const frame = makeFrame(coreJcs, contentJcs);
  const actual = {
    core_jcs_hex: coreJcs.toString('hex'),
    content_jcs_hex: contentJcs.toString('hex'),
    frame_hex: frame.toString('hex'),
    digest: `sha256:${createHash('sha256').update(frame).digest('hex')}`,
  };
  calculated[vector.name] = actual;
  if (!calculateOnly && JSON.stringify(actual) !== JSON.stringify(vector.expected)) {
    throw new Error(
      `${vector.name}: Node canonical bytes or digest differ from checked-in expected output`,
    );
  }
}

if (calculateOnly) {
  process.stdout.write(`${JSON.stringify(calculated, null, 2)}\n`);
} else {
  process.stderr.write(
    `RevisionCoreFormatV1 Node 24 oracle: ${manifest.valid.length} valid and ${manifest.rfc8785.length} RFC 8785 vectors passed\n`,
  );
}

function canonicalize(value) {
  if (value === null || typeof value === 'boolean' || typeof value === 'string') {
    return JSON.stringify(value);
  }
  if (typeof value === 'number') {
    if (!Number.isFinite(value)) {
      throw new Error('normalized JCS input contains a non-finite number');
    }
    return JSON.stringify(Object.is(value, -0) ? 0 : value);
  }
  if (Array.isArray(value)) {
    return `[${value.map(canonicalize).join(',')}]`;
  }
  if (typeof value === 'object') {
    return `{${Object.keys(value)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${canonicalize(value[key])}`)
      .join(',')}}`;
  }
  throw new Error(`unsupported normalized JCS value: ${typeof value}`);
}

function makeFrame(coreJcs, contentJcs) {
  return Buffer.concat([
    Buffer.from('pactrun.revision-content-digest\0', 'ascii'),
    u32be(1),
    Buffer.from('revision-core\0', 'ascii'),
    u64be(coreJcs.length),
    coreJcs,
    Buffer.from('runtime-content-closure\0', 'ascii'),
    u64be(contentJcs.length),
    contentJcs,
  ]);
}

function u32be(value) {
  const buffer = Buffer.alloc(4);
  buffer.writeUInt32BE(value);
  return buffer;
}

function u64be(value) {
  const buffer = Buffer.alloc(8);
  buffer.writeBigUInt64BE(BigInt(value));
  return buffer;
}

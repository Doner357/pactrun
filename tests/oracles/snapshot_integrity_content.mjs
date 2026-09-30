// Test-ID: PR-TEST-0019
// Verifies: PR-REQ-0080, PR-REQ-0195, PR-REQ-0202, PR-REQ-0203
import {createHash} from 'node:crypto';
import {readFileSync} from 'node:fs';

const vectorsPath = process.argv[2];
if (!vectorsPath) {
  throw new Error('usage: node snapshot_integrity.mjs <vectors.json> [--calculate]');
}

if (Number.parseInt(process.versions.node.split('.')[0], 10) !== 24) {
  throw new Error(`Snapshot integrity baseline oracle requires Node 24, found ${process.version}`);
}

const vectors = JSON.parse(readFileSync(vectorsPath, 'utf8'));
const calculateOnly = process.argv.includes('--calculate');
const calculated = {};

for (const vector of vectors.rfc8785) {
  const actual = canonicalize(vector.input);
  if (actual !== vector.expected) {
    throw new Error(`${vector.name}: RFC 8785 sample mismatch`);
  }
}

for (const vector of vectors.valid) {
  verifyBlobContents(vector);
  const manifestJcs = Buffer.from(canonicalize(vector.normalized_manifest), 'utf8');
  const frame = makeFrame(manifestJcs);
  const actual = {
    manifest_jcs_hex: manifestJcs.toString('hex'),
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
    `Snapshot integrity baseline Node 24 oracle: ${vectors.valid.length} valid and ${vectors.rfc8785.length} RFC 8785 vectors passed\n`,
  );
}

function verifyBlobContents(vector) {
  for (const [digest, bytesHex] of Object.entries(vector.blob_contents ?? {})) {
    if (!/^[0-9a-f]*$/.test(bytesHex) || bytesHex.length % 2 !== 0) {
      throw new Error(`${vector.name}: blob fixture is not lowercase hexadecimal`);
    }
    const actual = `sha256:${createHash('sha256').update(Buffer.from(bytesHex, 'hex')).digest('hex')}`;
    if (actual !== digest) {
      throw new Error(`${vector.name}: blob fixture digest mismatch for ${digest}`);
    }
  }

  const referenced = [
    ...vector.normalized_manifest.managed_bindings
      .filter((binding) => binding.state === 'bound')
      .map((binding) => binding.blob_digest),
    ...vector.normalized_manifest.service_content.map((content) => content.blob_digest),
  ];
  for (const digest of referenced) {
    if (!Object.hasOwn(vector.blob_contents ?? {}, digest)) {
      throw new Error(`${vector.name}: missing blob fixture for ${digest}`);
    }
  }
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

function makeFrame(manifestJcs) {
  return Buffer.concat([
    Buffer.from('pactrun.snapshot-integrity-digest\0', 'ascii'),
    Buffer.from('snapshot-integrity-manifest\0', 'ascii'),
    u64be(manifestJcs.length),
    manifestJcs,
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

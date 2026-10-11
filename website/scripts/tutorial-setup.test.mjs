import assert from 'node:assert/strict';
import test from 'node:test';
import {readFile, mkdir, mkdtemp, writeFile} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const source = await readFile(path.join(root, 'docs/introduction.md'), 'utf8');
const windows = process.platform === 'win32';
const language = windows ? 'powershell' : 'sh';
const snippet = source.match(new RegExp('^```' + language + '\\r?\\n([\\s\\S]*?)^```', 'm'))?.[1];
assert.ok(snippet, 'The introductory setup must be executable from its published code block');
const output = path.join(root, 'target/tutorial-setup-tests');
await mkdir(output, {recursive: true});

for (const existing of ['none', 'directory', 'file']) {
  test(`${language} tutorial setup: ${existing === 'none' ? 'fresh directory' : 'refuse existing ' + existing}`, async () => {
    const cwd = await mkdtemp(path.join(output, 'setup-'));
    const destination = path.join(cwd, 'pactrun-demo');
    const sentinel = existing === 'directory' ? path.join(destination, 'sentinel') : destination;
    if (existing === 'directory') await mkdir(destination);
    if (existing !== 'none') await writeFile(sentinel, 'preserve me');
    const script = windows ? `
$before = (Get-Location).Path
$preference = $ErrorActionPreference
$ok = $false
try {
${snippet}
$ok = $true
} catch { }
[ordered]@{ok=$ok; cwd=(Get-Location).Path; storage=$env:PACTRUN_STORAGE_ROOT; preferencePreserved=($preference -eq $ErrorActionPreference)} | ConvertTo-Json -Compress
` : `${snippet}
status=$?
printf '%s\\n' "$status" "$PWD" "$PACTRUN_STORAGE_ROOT"
`;
    const result = spawnSync(windows ? 'powershell.exe' : 'sh', windows
      ? ['-NoProfile', '-NonInteractive', '-Command', script] : ['-c', script],
    {cwd, env: {...process.env, PACTRUN_STORAGE_ROOT: 'unchanged-test-store'}, encoding: 'utf8', timeout: 15000});
    assert.ifError(result.error);
    assert.equal(result.status, 0, result.stderr);
    let observed;
    if (windows) {
      observed = JSON.parse(result.stdout.trim());
      assert.equal(observed.preferencePreserved, true);
    } else {
      const [status, workingDirectory, storage] = result.stdout.trim().split('\n');
      observed = {ok: status === '0', cwd: workingDirectory, storage};
    }
    assert.equal(observed.ok, existing === 'none', result.stderr);
    assert.equal(observed.cwd, existing === 'none' ? destination : cwd);
    assert.equal(observed.storage, existing === 'none' ? path.join(destination, 'store') : 'unchanged-test-store');
    if (existing !== 'none') assert.equal(await readFile(sentinel, 'utf8'), 'preserve me');
    else {
      const {stat} = await import('node:fs/promises');
      assert.ok((await stat(path.join(destination, 'pack'))).isDirectory());
      await assert.rejects(stat(path.join(destination, 'store')), {code: 'ENOENT'},
        'Tutorial setup selects a Store; Pactrun, not the shell snippet, provisions it');
    }
  });
}

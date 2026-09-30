import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';

// Execute the exact inline guard used by github-script, without network or tokens.
const workflow = readFileSync(new URL('../.github/workflows/pages.yml', import.meta.url), 'utf8');
const body = workflow.match(/\/\/ BEGIN PAGES GUARD([\s\S]*?)\/\/ END PAGES GUARD/)?.[1];
assert.ok(body, 'Missing executable Pages eligibility guard');
const guard = new (Object.getPrototypeOf(async function () {}).constructor)('github', 'context', 'core', body);

test('a passed docs-only gate deploys despite an intentionally skipped Windows ancestor', () => {
  const ci = readFileSync(new URL('../.github/workflows/ci.yml', import.meta.url), 'utf8');
  const caller = ci.split('\n  pages:\n')[1];
  const expression = caller.match(/if: \$\{\{ (.*?) \}\}/)[1];
  assert.match(expression, /always\(\)/);
  assert.match(expression, /!cancelled\(\)/);
  assert.match(expression, /needs\.gate\.result == 'success'/);
  const eligible = new Function('always', 'cancelled', 'needs', 'github', 'vars', 'return ' + expression);
  const base = {event_name: 'push', ref: 'refs/heads/main'};
  const enabled = {PACTRUN_PAGES_ENABLED: 'true'};
  assert.equal(eligible(() => true, () => false, {gate: {result: 'success'}, windows: {result: 'skipped'}}, base, enabled), true);
  for (const result of ['failure', 'cancelled', 'skipped', '']) {
    assert.equal(eligible(() => true, () => false, {gate: {result}}, base, enabled), false);
  }
  const passed = {gate: {result: 'success'}};
  assert.equal(eligible(() => true, () => true, passed, base, enabled), false);
  assert.equal(eligible(() => true, () => false, passed, {...base, event_name: 'pull_request'}, enabled), false);
  assert.equal(eligible(() => true, () => false, passed, {...base, ref: 'refs/heads/develop'}, enabled), false);
  assert.equal(eligible(() => true, () => false, passed, base, {PACTRUN_PAGES_ENABLED: 'false'}), false);
  assert.match(workflow, /if: \$\{\{ always\(\) && !cancelled\(\)/);
});

for (const [name, eventName, ref, current, expected] of [
  ['current main push', 'push', 'refs/heads/main', 'tested', 'true'],
  ['superseded main push', 'push', 'refs/heads/main', 'newer', 'false'],
  ['pull request', 'pull_request', 'refs/heads/main', 'tested', 'false'],
  ['develop push', 'push', 'refs/heads/develop', 'tested', 'false'],
  ['manual event', 'workflow_dispatch', 'refs/heads/main', 'tested', 'false'],
]) {
  test(name, async () => {
    const outputs = {};
    await guard({rest: {repos: {getBranch: async (args) => {
      assert.deepEqual(args, {owner: 'owner', repo: 'repo', branch: 'main'});
      return {data: {commit: {sha: current}}};
    }}}}, {repo: {owner: 'owner', repo: 'repo'}, eventName, ref, sha: 'tested'},
    {setOutput: (key, value) => { outputs[key] = value; }, notice: () => {}});
    assert.equal(outputs.publish, expected);
  });
}

test('API failure cannot approve deployment', async () => {
  const outputs = {};
  await assert.rejects(guard({rest: {repos: {getBranch: async () => { throw new Error('API failure'); }}}},
    {repo: {owner: 'owner', repo: 'repo'}},
    {setOutput: (key, value) => { outputs[key] = value; }}), /API failure/);
  assert.notEqual(outputs.publish, 'true');
});

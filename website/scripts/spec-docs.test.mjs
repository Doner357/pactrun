import assert from 'node:assert/strict';
import {readFile, readdir} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import test from 'node:test';
import {readDocuments} from '../plugins/text-docs/index.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const docs = await readDocuments(path.join(root, 'docs'));

export function assertSpecOnlyDefinitions(documents) {
  const seen = new Set();
  for (const [name, body] of documents) {
    for (const match of body.matchAll(/^#{1,6} (PR-REQ-\d+)\b/gm)) {
      assert.ok(name.startsWith('spec/'), 'Definition outside Spec: ' + name);
      assert.ok(!seen.has(match[1]), 'Duplicate definition: ' + match[1]);
      seen.add(match[1]);
    }
  }
  return seen;
}

test('product requirements have unique definitions only under Spec', () => {
  assert.ok(assertSpecOnlyDefinitions(docs).size > 0);
});

test('outside and duplicate definitions fail; reference links are allowed', () => {
  assert.throws(() => assertSpecOnlyDefinitions([['development/a.md', '### PR-REQ-9999 - Example']]));
  assert.throws(() => assertSpecOnlyDefinitions([['spec/a.md', '### PR-REQ-9999 - Example'], ['spec/b.md', '### PR-REQ-9999 - Duplicate']]));
  assertSpecOnlyDefinitions([['development/a.md', '[PR-REQ-9999](../spec/a.md)']]);
});

test('catalog matches every requirement-bearing document and its original status', () => {
  const catalog = docs.find(([name]) => name === 'spec/catalog.md')[1];
  for (const [name, body] of docs) {
    if (!/^### PR-REQ-\d+/m.test(body)) continue;
    const status = body.match(/\*\*Status: ([\s\S]*?)\*\*/)?.[1].replace(/\s+/g, ' ').trim();
    assert.ok(status, 'Missing status: ' + name);
    const target = name.slice('spec/'.length);
    const line = catalog.split('\n').find(value => value.includes('](' + target + ')'));
    assert.ok(line, 'Missing catalog entry: ' + name);
    assert.ok(line.includes('| ' + status + ' |'), 'Stale catalog status: ' + name);
    assert.ok(body.includes('<!-- spec-navigation:start -->'), 'Missing reading map: ' + name);
  }
});

test('usage guides are placeholders, not completed tutorials', () => {
  const names = ['agents/use-pactrun.md', 'agents/author-packs.md', 'agents/integrate-hooks.md'];
  for (const [name, body] of docs) {
    if (!names.includes(name) && !name.startsWith('pactrun-users/') && !name.startsWith('package-authors/')) continue;
    assert.match(body, /placeholder|reserves the\s+documentation structure/i, name);
    assert.doesNotMatch(body, /^### PR-REQ-\d+/m, name);
  }
});

test('main sidebar uses Spec and Development instead of compatibility locations', async () => {
  const sidebar = await readFile(path.join(root, 'website/sidebars.ts'), 'utf8');
  assert.doesNotMatch(sidebar, /['"]pactrun-developers\//);
  assert.doesNotMatch(sidebar, /['"]agents\//);
  assert.match(sidebar, /spec\/index/);
  assert.match(sidebar, /development\/index/);
});

test('M5 integration remains distinct from separately approved M6 work', async () => {
  const roadmap = docs.find(([name]) => name === 'development/implementation-roadmap.md')[1];
  const baseline = docs.find(([name]) => name === 'development/design-notes/m5-migration-implementation-baseline.md')[1];
  const status = docs.find(([name]) => name === 'development/m5-implementation-status.md')[1];
  assert.match(roadmap, /### M5 - Migration\s+\*\*State: Implemented and integrated into develop\.\*\*/);
  assert.match(roadmap, /### M6 - Recovery\s+\*\*State: Complete\.\*\*/);
  assert.match(baseline, /Approved scope and implementation decisions, 2026-09-13/);
  assert.match(status.replace(/\s+/g, ' '), /approved M5 scope is implemented, verified and integrated into local develop/);
  assert.match(status, /Current production storage is V6/);
  assert.match(status.replace(/\s+/g, ' '), /Hook-backed chains and operator file inputs are integrated into the existing Migration Run path/);
  assert.match(status.replace(/\s+/g, ' '), /No ServiceStorage representation\/runtime, generalized M6 recovery/);
  const readme = await readFile(path.join(root, 'README.md'), 'utf8');
  assert.match(readme.replace(/\s+/g, ' '), /target-qualified operator file inputs, declarative and Hook-backed chains/);
  assert.doesNotMatch(readme, /in-progress M5|not full M5 completion|remain pending; this is/);
});

test('roadmap distinguishes historical V4 introduction from the current schema', () => {
  const roadmap = docs.find(([name]) => name === 'development/implementation-roadmap.md')[1];
  assert.doesNotMatch(roadmap.replace(/\s+/g, ' '), /PersistenceSchemaV4 as the current canonical implemented internal schema/);
});

test('current entries reflect M7 integration while preserving bounded M6 history', async () => {
  const document = name => docs.find(([file]) => file === 'development/' + name)[1];
  const roadmap = document('implementation-roadmap.md');
  const milestones = [...roadmap.matchAll(/^### (M6(?:\.5)?|M7|M8) - /gm)].map(match => match[1]);
  assert.deepEqual(milestones, ['M6', 'M6.5', 'M7']);
  assert.match(roadmap, /### M6 - Recovery\s+\*\*State: Complete\.\*\*/);
  assert.match(roadmap, /### M6\.5 - ServiceStorage\s+\*\*State: Complete\./);
  assert.match(roadmap, /### M7 - Cleanup and deletion\s+\*\*State: Implemented, verified and integrated into local develop\./);
  const current = roadmap.split('## Current baseline')[1].split('## Milestone states')[0].replace(/\s+/g, ' ');
  assert.match(current, /PersistenceSchemaV8\]\([^)]*\) is the integrated `develop` persistence baseline, with explicit exact-V7 upgrade only/);
  assert.match(current, /M7 Cleanup execution is implemented, verified and integrated into local `develop`/);
  assert.match(current, /M8 is rejected and archived; no next numbered milestone is selected/);
  assert.doesNotMatch(current, /Cleanup execution remains Proposed/);
  const agentEntry = docs.find(([file]) => file === 'agents/index.md')[1].replace(/\s+/g, ' ');
  assert.match(agentEntry, /M5, bounded M6, M6\.5 ServiceStorage and M7 Cleanup\/deletion are implemented and integrated into local `develop`/);
  assert.match(agentEntry, /current persistence baseline is V8, with explicit exact-V7 upgrade only/);
  for (const entry of [current, agentEntry]) {
    assert.match(entry, /M8 is rejected and archived; no next numbered milestone is selected/);
    assert.match(entry, /Release-readiness work has no assigned start/);
    assert.match(entry, /integration does not authorize publication/);
    assert.doesNotMatch(entry, /M6\.5 is next and remains Proposed/);
  }
  assert.doesNotMatch(current, /Migration and Cleanup execution remain deferred/);
  assert.doesNotMatch(roadmap.replace(/\s+/g, ' '), /Hook and operator-input execution remain pending/);

  const baseline = document('design-notes/m6-recovery-implementation-baseline.md').replace(/\s+/g, ' ');
  assert.match(baseline, /Approved bounded M6 scope/);
  assert.match(baseline, /S0 review complete/);
  assert.match(baseline, /Decision: retain exact V6/);
  assert.match(baseline, /Do not silently alter exact V6/);
  const closeout = document('m6-implementation-status.md').replace(/\s+/g, ' ');
  assert.match(closeout, /Status: Complete\. Bounded M6 S0-S4 is implemented and integrated into local develop/);
  assert.match(closeout, /explicitly without push/);
  assert.match(document('next-milestone.md'), /M7 is integrated; M8 is rejected/);
  assert.doesNotMatch(document('next-milestone.md'), /M6 is `In progress`/);
  const alignment = document('design-notes/service-storage-staged-design-alignment.md').replace(/\s+/g, ' ');
  assert.match(alignment, /before approving the storage schema/);
  assert.match(alignment, /broader taxonomy.*remain deferred and do not block/);

  for (const entry of ['index.md', 'reading-paths.md', 'next-milestone.md', 'implementation-guidance.md', 'design-notes/service-storage-semantic-baseline.md']) {
    assert.match(document(entry), /service-storage-staged-design-alignment\.md/, entry);
    assert.match(document(entry), /m6-recovery-implementation-baseline\.md/, entry);
  }
  const sidebar = await readFile(path.join(root, 'website/sidebars.ts'), 'utf8');
  assert.match(sidebar, /development\/design-notes\/service-storage-staged-design-alignment/);
  assert.match(sidebar, /development\/design-notes\/m6-recovery-implementation-baseline/);
  assert.match(sidebar, /development\/m6-implementation-status/);
});

test('M6.5 design direction preserves the separate runtime and Freeze gates', async () => {
  const proposals = [
    'contracts/revision-core-format-v2.md',
    'contracts/pack-source-yaml-v2.md',
    'contracts/hook-protocol-v2.md',
    'persistence/persistence-schema-v7.md',
    'execution/m6-5-service-storage-execution.md',
    'behavior/m6-5-service-storage-command-reference.md',
  ];
  const baseline = docs.find(([name]) => name === 'development/design-notes/m6-5-servicestorage-baseline.md')[1];
  assert.match(baseline.replace(/\s+/g, ' '), /S1-S7 may proceed/);
  const core = docs.find(([file]) => file === 'spec/contracts/revision-core-format-v2.md')[1];
  assert.match(core, /A source named by reuse, reattach or transform is consumed/);
  for (const name of proposals) {
    const body = docs.find(([file]) => file === 'spec/' + name)?.[1];
    assert.ok(body, 'Missing S0 proposal: ' + name);
    if (name === 'contracts/revision-core-format-v2.md' || name === 'contracts/hook-protocol-v2.md') {
      assert.match(body, /Status: Frozen normative Package contract specification/);
      assert.match(body, /m6-5-format-activation-review\.md/);
    } else if (name === 'contracts/pack-source-yaml-v2.md') {
      assert.match(body, /Candidate normative Package authoring contract; versioned, non-Frozen/);
    } else {
      assert.match(body, /Status: Implemented normative/);
    }
    assert.match(baseline, new RegExp(name.replaceAll('.', '\\.')));
    const rules = body.split(/^### PR-REQ-\d+/m).slice(1);
    assert.ok(rules.length > 0, 'No proposed requirements: ' + name);
    for (const rule of rules) assert.match(rule, /\*\*Verification:/);
    if (name === 'contracts/hook-protocol-v2.md') {
      assert.match(rules[0], /Codec, authority and ordinary-runtime evidence/);
      assert.match(rules[1], /Protocol and target-runtime evidence/);
      assert.match(body, /no persisted proposal flag exists/);
      assert.match(body.replace(/\s+/g, ' '), /Codec tests alone are not completed Migration target-publication evidence/);
    }
    if (name === 'execution/m6-5-service-storage-execution.md') {
      assert.match(rules[0], /\*\*Verification: PR-TEST-0354, PR-TEST-0355, PR-TEST-0366, PR-TEST-0367, PR-TEST-0368, PR-TEST-0370, PR-TEST-0371, PR-TEST-0373, PR-TEST-0374, PR-TEST-0375, PR-TEST-0376, PR-TEST-0377, PR-TEST-0378, PR-TEST-0380, PR-TEST-0381, PR-TEST-0382, PR-TEST-0383, PR-TEST-0386, PR-TEST-0387, PR-TEST-0388\.\*\*/);
      assert.match(rules[0], /Filesystem observation substrate coverage/);
      assert.match(rules[0], /final-source closeout results/);
      assert.match(rules[1], /M6\.5 association\/lifetime evidence only/);
      assert.match(rules[1].replace(/\s+/g, ' '), /do not implement M7 Cleanup/);
    }
    if (name === 'behavior/m6-5-service-storage-command-reference.md') {
      assert.match(body, /available in the integrated M6\.5 baseline/);
      assert.match(body, /CLI runtime coverage/);
      assert.match(body, /production V2 installation, cross-version retention and explicit reattachment/);
    }
  }
  const current = await readFile(path.join(root, 'src/persistence/sqlite_revision_store.rs'), 'utf8');
  assert.match(current, /pub\(crate\) const SCHEMA_VERSION: i64 = 9;/);
  const lifecycle = docs.find(([name]) => name === 'spec/persistence/persistence-schema-v9.md')[1];
  assert.match(lifecycle, /Only explicit exact-V8 to V9 upgrade is supported/);
  const m7 = docs.find(([name]) => name === 'development/m7-implementation-status.md')[1];
  assert.match(m7, /Implemented, verified and integrated into local develop/);
  assert.match(m7, /513dbf3b7296f01fed2ae2fc4ddf1e4e9a36f4cec34b63122328d1b4e4af0647/);
  assert.match(m7.replace(/\s+/g, ' '), /Both original counterexamples now pass/);
  const v8 = docs.find(([name]) => name === 'spec/persistence/persistence-schema-v8.md')[1];
  assert.match(v8, /Approved M7 internal contract; non-Frozen/);
  const roadmap = docs.find(([name]) => name === 'development/implementation-roadmap.md')[1];
  assert.match(roadmap, /### M6\.5 - ServiceStorage\s+\*\*State: Complete\./);
  const closeout = docs.find(([name]) => name === 'development/m6-5-implementation-status.md')[1];
  assert.match(closeout.replace(/\s+/g, ' '), /Status: Complete\. M6\.5 S1-S7 is implemented and integrated into local develop/);
  assert.match(closeout, /cf7562d77201d1e0cf29f298101cc4116a51a72b/);
  assert.match(closeout, /without push/);
});

test('release readiness separates work order from publication and implementation', async () => {
  const document = name => docs.find(([file]) => file === name)?.[1];
  const plan = document('development/release-readiness.md');
  const policy = document('spec/foundations/product-versioning-and-compatibility.md');
  assert.ok(plan);
  assert.ok(policy);
  const normalizedPlan = plan.replace(/\s+/g, ' ');
  const normalizedPolicy = policy.replace(/\s+/g, ' ');
  assert.match(normalizedPlan, /Required before formal release; relative work order assigned, implementation pending/);
  assert.match(normalizedPlan, /Retirement of M8 does not trigger these tasks or a release/);
  assert.match(normalizedPlan, /not a mandatory milestone-to-release chain/);
  for (const work of [
    'Versioning mechanism design and implementation',
    'Baseline reorganization and consolidation',
    'Internal testing, evaluation and correction',
    'Release mechanism design and implementation',
  ]) assert.ok(plan.includes('| ' + work + ' |'), work);
  assert.match(normalizedPlan, /Formal publication is a separate controlled action/);
  assert.match(normalizedPlan, /does not implement versioning, change Cargo's product version, reset data/);
  assert.match(normalizedPlan, /no field name or version-range syntax has been approved yet/);
  assert.match(normalizedPolicy, /Approved formal-release compatibility design; implementation and baseline consolidation pending/);
  assert.deepEqual([...policy.matchAll(/^### (PR-REQ-\d+)/gm)].map(match => match[1]), [
    'PR-REQ-0329', 'PR-REQ-0330', 'PR-REQ-0331', 'PR-REQ-0332', 'PR-REQ-0333',
  ]);
  assert.equal((policy.match(/\*\*Verification: Pending automated coverage\.\*\*/g) ?? []).length, 5);
  assert.match(normalizedPolicy, /earlier published external contracts of that Major/);
  assert.match(normalizedPolicy, /does not promise compatibility with a previous Major's external formats/);
  assert.match(normalizedPolicy, /Development iterations are not earlier formal product releases/);
  assert.match(normalizedPolicy, /Data acceptance MUST depend on full conformance to the current supported formal contracts/);
  assert.match(normalizedPolicy, /conforming data MUST NOT be rejected solely because of its development provenance/);
  assert.match(normalizedPolicy, /MUST NOT require a development-generation marker or discriminator merely to identify and exclude development data/);
  assert.match(normalizedPolicy, /Conformance MUST include applicable identity, encoding, reference, invariant and semantic checks/);
  assert.match(normalizedPolicy, /shipping program MUST NOT retain readers, migration chains, version dispatch or special cases solely to support superseded development contracts/);
  assert.doesNotMatch(normalizedPolicy, /MUST distinguish old development data reliably/);
  assert.doesNotMatch(normalizedPlan, /explicit old-generation rejection/);
  assert.match(normalizedPolicy, /MUST remain at product version 0\.1\.0/);
  assert.match(normalizedPolicy, /MUST NOT automatically publish or label the product 1\.0\.0/);
  assert.match(normalizedPolicy, /MUST NOT perform another format reset/);
  const roadmap = document('development/implementation-roadmap.md').replace(/\s+/g, ' ');
  assert.match(roadmap, /Release-readiness prerequisites \(publication gates\)/);
  assert.match(roadmap, /not tasks automatically scheduled by milestone completion or retirement/);
  assert.match(roadmap, /### M7 - Cleanup and deletion \*\*State: Implemented, verified and integrated into local develop\./);
  assert.match(roadmap, /M8 was rejected and removed from the active roadmap/);
  assert.doesNotMatch(roadmap, /### M8 - /);
  const sidebar = await readFile(path.join(root, 'website/sidebars.ts'), 'utf8');
  assert.match(sidebar, /spec\/foundations\/product-versioning-and-compatibility/);
  assert.match(sidebar, /development\/release-readiness/);
});

test('M8 rejection retires Recipe obligations but preserves identity and internal authoring boundaries', async () => {
  const document = name => docs.find(([file]) => file === name)[1];
  const history = document('development/history/m8-recipes-rejected.md');
  const runtime = document('spec/contracts/recipes-and-runtime-content.md');
  const authoring = document('spec/contracts/authoring-model.md');
  const handoff = document('development/next-milestone.md');
  assert.match(history, /Historical rejected proposal/);
  assert.match(history, /2026-09-16; not deferred implementation/);
  for (const reason of ['Open-ended Revision variation', 'No isolated execution environment',
    'Migration declarations are only part of the problem', 'Hook behavior can grow combinatorially',
    'Lifecycle maintenance outweighs installation convenience', 'No demonstrated need']) {
    assert.ok(history.includes(reason), reason);
  }
  assert.match(history.replace(/\s+/g, ' '), /does not ban networking/);
  assert.match(history.replace(/\s+/g, ' '), /public, versioned Candidate entry point.*deferred until a concrete need exists/);
  for (const id of ['0137', '0139', '0140']) {
    const section = runtime.split('### PR-REQ-' + id + ' - ')[1]?.split('\n### ')[0];
    assert.ok(section, id);
    assert.match(section, /Retired: M8 rejected on 2026-09-16/);
    assert.doesNotMatch(section, /Verification: Pending automated coverage/);
    assert.ok(history.includes('PR-REQ-' + id), id);
  }
  assert.match(runtime.replace(/\s+/g, ' '), /same canonical Revision Core and the same owned runtime content produce the same `RevisionContentDigest`/);
  assert.match(runtime, /Pactrun MUST materialize runtime content/);
  assert.match(authoring.replace(/\s+/g, ' '), /Every authoring frontend MUST ultimately produce a `RevisionCandidate`/);
  assert.match(authoring, /is not a public API/);
  assert.match(handoff, /No next numbered milestone is selected/);
  const sidebar = await readFile(path.join(root, 'website/sidebars.ts'), 'utf8');
  assert.match(sidebar, /development\/history\/m8-recipes-rejected/);
});

test('completion plan preserves agreed order, bounded slices and pending implementation', async () => {
  const document = name => docs.find(([file]) => file === name)[1];
  const plan = document('development/product-completion-milestones.md');
  const normalized = plan.replace(/\s+/g, ' ');
  assert.match(normalized, /Approved planning scope and work order, recorded on 2026-09-17/);
  assert.match(normalized, /Detailed design and runtime implementation are pending/);
  assert.deepEqual([...plan.matchAll(/^## \d\. (.+)$/gm)].map(match => match[1]), [
    'Managed Object Lifecycle and GC',
    'Snapshot Capacity and Restore Workflow',
    'Shell Adapter / Loader',
    'Machine-readable CLI Output',
    'Versioning and Baseline Consolidation',
  ]);
  assert.match(normalized, /Versioning is last, not a required preliminary design milestone/);
  const lifecycle = plan.split('## 1. ')[1].split('## 2. ')[0];
  assert.match(lifecycle, /### Artifact export slice/);
  for (const object of ['Snapshot', 'Revision', 'Run record', 'Run Artifact']) {
    assert.ok(lifecycle.includes('| ' + object + ' |'), object);
  }
  const snapshot = plan.split('## 2. ')[1].split('## 3. ')[0];
  assert.match(snapshot, /### Create-and-restore convenience slice/);
  assert.match(normalized, /Current PR-REQ-0293 remains binding until its owning contract is revised/);
  assert.match(normalized, /Do not automatically delete the new Instance on Restore failure/);
  assert.match(normalized, /Normal zero-status script exit defaults to a request for successful completion/);
  assert.match(normalized, /Never auto-clear Open risk/);
  assert.match(normalized, /Command semantics and permissions remain identical to human use/);
  assert.match(normalized, /Accept fully conforming data irrespective of development provenance/);
  assert.match(normalized, /This planning record changes no executable, stored data or current runtime limit/);
  for (const entry of ['development/index.md', 'development/implementation-roadmap.md',
    'development/next-milestone.md', 'development/release-readiness.md',
    'agents/index.md', 'agents/develop-pactrun.md']) {
    assert.ok(document(entry).includes('product-completion-milestones.md'), entry);
  }
  const sidebar = await readFile(path.join(root, 'website/sidebars.ts'), 'utf8');
  assert.match(sidebar, /development\/product-completion-milestones/);
});

test('contributor rules preserve product naming, rationale and branch target discipline', async () => {
  const policy = docs.find(([name]) => name === 'development/development-and-verification.md')[1];
  const normalized = policy.replace(/\s+/g, ' ');
  assert.match(normalized, /New functions and tests MUST use product concepts, behavior or invariants/);
  assert.match(normalized, /Existing occurrences are deferred to Versioning and Baseline Consolidation/);
  assert.match(normalized, /Actual protocol\/format\/schema version identities and stable PR-REQ\/PR-TEST IDs/);
  assert.match(normalized, /Contributors SHOULD record the rationale for each new or revised design rule/);
  assert.match(normalized, /If the original reason is unknown, say it was not recorded/);
  assert.match(normalized, /Rationale is informative, not a second normative contract/);
  assert.match(normalized, /Determine the version line being changed and its integration targets before choosing a branch name/);
  for (const prefix of ['feature/', 'fix/', 'docs/', 'release/', 'hotfix/']) {
    assert.ok(policy.includes('`' + prefix + '`'), prefix);
  }
  assert.match(normalized, /integrate a production hotfix there as Git Flow prescribes/);
  assert.match(normalized, /No historical branch renaming or history rewriting is implied/);
  const contributing = await readFile(path.join(root, 'CONTRIBUTING.md'), 'utf8');
  assert.match(contributing, /git-flow-and-topic-naming/);
  assert.match(contributing, /product-oriented-functions-and-tests/);
});

test('validation policy selects by impact while preserving full-suite gates and honest evidence', async () => {
  const policy = docs.find(([name]) => name === 'development/development-and-verification.md')[1];
  const normalized = policy.replace(/\s+/g, ' ');
  assert.match(normalized, /Routine edits MUST NOT automatically run the complete product suite/);
  assert.match(normalized, /No full Rust product suite by default/);
  assert.match(normalized, /inspect `include_str!`, extracted SQL/);
  assert.match(normalized, /An explicit request for full CI always overrides a narrower default/);
  assert.match(normalized, /stable runtime-bearing milestone integration candidate and for formal release/);
  assert.match(normalized, /A commit SHA change alone is not a reason to repeat full CI/);
  assert.match(normalized, /Report full CI as `Not run` when only narrower checks ran/);
  assert.match(normalized, /filter that executes zero intended tests is not proof/);
  assert.match(normalized, /relevant production code, tests, fixtures, dependencies, toolchain\/build settings/);
  assert.match(normalized, /not a new automatic cache or filename-based skip system/);
  const contribution = await readFile(path.join(root, 'CONTRIBUTING.md'), 'utf8');
  assert.match(contribution, /risk-based-validation-scope/);
  assert.doesNotMatch(contribution, /Run `cargo xtask ci` for the same ordered checks/);
  const roadmap = docs.find(([name]) => name === 'development/implementation-roadmap.md')[1];
  assert.match(roadmap.replace(/\s+/g, ' '), /stable runtime-bearing milestone integration candidate/);
  const readiness = docs.find(([name]) => name === 'development/release-readiness.md')[1];
  assert.match(readiness.replace(/\s+/g, ' '), /documentation-only updates do not trigger the full product suite/);
});

async function rustFiles(directory) {
  const files = [];
  for (const entry of await readdir(directory, {withFileTypes: true})) {
    const name = path.join(directory, entry.name);
    if (entry.isDirectory()) files.push(...await rustFiles(name));
    else if (entry.isFile() && name.endsWith('.rs')) files.push(name);
  }
  return files;
}

test('embedded Markdown contracts use canonical Spec paths, not forwarding pages', async () => {
  for (const name of await rustFiles(path.join(root, 'src'))) {
    const source = await readFile(name, 'utf8');
    for (const match of source.matchAll(/include_str!\(\s*"([^"]*docs\/[^"]+\.md)"/g)) {
      assert.ok(match[1].includes('/docs/spec/'), 'Noncanonical embedded contract: ' + name);
      const contract = await readFile(path.resolve(path.dirname(name), match[1]), 'utf8');
      assert.match(contract, /\*\*Status:/);
    }
  }
});

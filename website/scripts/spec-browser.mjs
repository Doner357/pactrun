import assert from 'node:assert/strict';
import {mkdir, writeFile} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';
import {readDocuments} from '../plugins/text-docs/index.mjs';

const [base, output] = process.argv.slice(2);
if (!base || !output || !process.env.PLAYWRIGHT_PACKAGE) throw new Error('Provide base URL, evidence directory and PLAYWRIGHT_PACKAGE');
const {chromium} = await import(pathToFileURL(process.env.PLAYWRIGHT_PACKAGE).href);
await mkdir(output, {recursive: true});
const browser = await chromium.launch({headless: true});
const page = await browser.newPage({viewport: {width: 1440, height: 1000}});
const errors = [];
page.on('pageerror', error => errors.push(error.message));
const root = base.replace(/\/$/, '');
const url = route => root + '/' + route.replace(/^\//, '');
const chapters = ['core', 'packages', 'instances', 'operations', 'snapshots', 'migrations', 'lifecycle', 'interfaces', 'storage'];
const site = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const checks = [];
const visited = [];
try {
  for (const edition of ['1.0.0-alpha.4', 'next']) {
    const response = await page.request.get(url(edition + '/document-catalog.json'));
    assert.ok(response.ok());
    const catalog = await response.json();
    const specs = catalog.pages.filter(item => item.source.startsWith('spec/'));
    const source = await readDocuments(edition === 'next' ? path.resolve(site, '../docs/spec') : path.join(site, 'versioned_docs/version-' + edition, 'spec'));
    assert.deepEqual(specs.map(item => item.source).sort(), source.filter(([name]) => name.endsWith('.md')).map(([name]) => 'spec/' + name).sort());
    const expected = source.flatMap(([, text]) => [...text.matchAll(/^### (PR-REQ-\d+)/gm)].map(match => match[1])).sort();
    assert.deepEqual(specs.flatMap(item => item.requirements).sort(), expected);
    await page.goto(url(edition + '/spec/'), {waitUntil: 'networkidle'});
    for (const chapter of chapters) assert.equal(await page.locator(`.theme-doc-markdown h2 a[href$="/spec/${chapter}/"]`).count(), 1);
    await page.screenshot({path: path.join(output, edition + '-overview.png'), fullPage: false});
    for (const spec of specs) {
      await page.goto(url(spec.url), {waitUntil: 'networkidle'});
      const article = page.locator('.theme-doc-markdown');
      await article.getByRole('heading', {name: spec.title, exact: true}).waitFor();
      const text = await article.innerText();
      assert.doesNotMatch(text, /\bM[0-8](?:\.\d)?(?:-[A-Z])?\b|\bmilestone\b|\bpre-E\b|\bnormative\b|\binformative\b/i, spec.source);
      assert.equal(await article.locator('a[href*="/development/"]').count(), 0, spec.source);
      const returnLinks = await page.locator('.pagination-nav a[href]').evaluateAll(items => items.map(item => item.getAttribute('href')));
      assert.ok(returnLinks.every(href => href.includes('/spec/')), spec.source + ': navigation leaves Spec');
      assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), spec.source);
      visited.push(edition + ':' + spec.source);
    }
    checks.push(edition + ': every Spec page renders without stage/classification prose, development prerequisites or page overflow');
  }
  for (const [legacy, expected] of [
    ['foundations/identity-and-state#pr-req-0025---opaque-instancestateversion', '/instances/identity-state#pr-req-0025'],
    ['foundations/identity-and-state#pr-req-0231---sqlite-persistence-ownership-and-bootstrap', '/storage/revision-records#pr-req-0231'],
    ['foundations/identity-and-state#servicestorage-backed-semantic-closure', '/instances/service-resources'],
    ['persistence/persistence-schema-v3?from=bookmark#pr-req-0270---persistence-migration-to-v3', '/storage/schema?from=bookmark#pr-req-0299'],
    ['contracts/recipes-and-runtime-content#retired-recipe-proposal-and-retained-identity-rule', '/packages/runtime-content#reproducibility'],
  ]) {
    await page.goto(url('1.0.0-alpha.4/spec/' + legacy), {waitUntil: 'networkidle'});
    await page.waitForURL(target => target.href.includes(expected));
    const hash = new URL(page.url()).hash;
    if (hash) assert.equal(await page.locator('[id="' + hash.slice(1) + '"]').count(), 1);
  }
  checks.push('legacy split-page and retired-schema bookmarks preserve the subject, query and valid target anchor');

  const response = await page.request.get(url('1.0.0-alpha.4/agent-docs/spec/foundations/identity-and-state.md'));
  assert.ok(response.ok());
  const text = await response.text();
  for (const id of ['PR-REQ-0011', 'PR-REQ-0025', 'PR-REQ-0231', 'PR-REQ-0241']) assert.ok(text.includes(id));
  for (const target of ['spec/packages/identity.md', 'spec/instances/identity-state.md', 'spec/storage/revision-records.md']) assert.ok(text.includes('Source: docs/' + target));
  checks.push('legacy machine-text aggregate retains all split owners in the selected version');

  await page.goto(url('1.0.0-alpha.4/search?q=PR-REQ-0025'), {waitUntil: 'networkidle'});
  await page.waitForFunction(() => document.querySelector('[role="status"]')?.textContent.includes('matching documents'));
  assert.ok((await page.locator('.search-results h2 a').first().getAttribute('href')).includes('/spec/instances/identity-state#pr-req-0025'));
  checks.push('exact requirement search locates the new owning topic');

  await page.setViewportSize({width: 390, height: 844});
  for (const chapter of chapters) {
    await page.goto(url('1.0.0-alpha.4/spec/' + chapter + '/'), {waitUntil: 'networkidle'});
    assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), chapter);
  }
  await page.goto(url('1.0.0-alpha.4/spec/core/objects'), {waitUntil: 'networkidle'});
  await page.screenshot({path: path.join(output, 'mobile-objects.png'), fullPage: false});
  await page.getByRole('button', {name: 'Toggle navigation bar'}).click();
  await page.locator('.navbar-sidebar__items--show-secondary').waitFor();
  await page.waitForFunction(() => {
    const panel = document.querySelector('.navbar-sidebar__item:not([inert])');
    const bounds = panel?.getBoundingClientRect();
    return bounds && Math.abs(bounds.left) < 1 && bounds.width > 0 && bounds.right <= innerWidth + 1;
  });
  const menu = await page.locator('.navbar-sidebar__item:not([inert])').innerText();
  for (const title of ['System and Core Concepts', 'Packages and Revisions', 'Instances and Data', 'Operations and Execution', 'Snapshots and Restore', 'Migration', 'Lifecycle and Recovery', 'Hooks and Integration Interfaces', 'Compatibility and Storage']) assert.ok(menu.includes(title), title);
  await page.screenshot({path: path.join(output, 'mobile-navigation.png'), fullPage: false, animations: 'disabled'});
  checks.push('all nine topic entrances fit the mobile viewport');
  assert.deepEqual(errors, []);
  const receipt = {status: 'Passed', base, checks, visited, pageErrors: errors};
  await writeFile(path.join(output, 'receipt.json'), JSON.stringify(receipt, null, 2));
  console.log(JSON.stringify({status: 'Passed', pagesVisited: visited.length, checks}, null, 2));
} catch (error) {
  await page.screenshot({path: path.join(output, 'failure.png'), fullPage: false});
  await writeFile(path.join(output, 'failure.json'), JSON.stringify({url: page.url(), visited, checks, errors, message: String(error)}, null, 2));
  console.error('Failed at', page.url(), 'after', visited.length, 'pages');
  throw error;
} finally { await browser.close(); }

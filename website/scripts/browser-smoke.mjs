import assert from 'node:assert/strict';
import {mkdir, writeFile} from 'node:fs/promises';
import path from 'node:path';
import {pathToFileURL} from 'node:url';
const [base = 'http://127.0.0.1:3000/', output = 'browser-evidence'] = process.argv.slice(2);
if (!process.env.PLAYWRIGHT_PACKAGE) throw new Error('Set PLAYWRIGHT_PACKAGE to an installed Playwright index.mjs');
const {chromium} = await import(pathToFileURL(process.env.PLAYWRIGHT_PACKAGE).href);
await mkdir(output, {recursive: true});
const browser = await chromium.launch({headless: true});
const page = await browser.newPage({viewport: {width: 1440, height: 1000}});
const errors = [];
page.on('pageerror', error => errors.push(error.message));
const url = route => base.replace(/\/$/, '') + '/' + route;
const checks = [];
async function ready() {
  await page.waitForFunction(() => document.querySelector('[role="status"]')?.textContent.includes('matching documents'));
}
async function openFilters() {
  if (!await page.locator('.search-filter-panel').evaluate(el => el.open))
    await page.locator('.search-filter-panel summary').click();
}
try {
  await page.goto(url(''), {waitUntil: 'networkidle'});
  await page.getByRole('heading', {name: 'Manage Packs and Instances', exact: true}).waitFor();
  await page.screenshot({path: path.join(output, 'home-desktop.png'), fullPage: true});
  checks.push('home task navigation renders');

  await page.goto(url('commands'), {waitUntil: 'networkidle'});
  assert.ok((await page.locator('main').innerText()).includes('pactrun snapshot restore'));
  await page.getByRole('link', {name: 'Invoke: parameters, timeouts, defaults, and examples'}).click();
  await page.getByRole('heading', {name: 'Invoke command reference', exact: true}).waitFor();
  assert.ok((await page.locator('.theme-doc-markdown').innerText()).includes('--action-timeout-ms'));
  checks.push('generated overview links directly to complete Invoke reference');

  await page.goto(url('search?q=PR-REQ-0258&audience=Authors'), {waitUntil: 'networkidle'});
  await ready();
  const owner = page.locator('.search-results li h2 a').first();
  assert.equal(await owner.innerText(), 'Pack Source Format');
  const href = await owner.getAttribute('href');
  assert.ok(href.includes('#pr-req-0258-'));
  await owner.click();
  await page.waitForFunction(() => {
    const target = document.getElementById(decodeURIComponent(location.hash.slice(1)));
    if (!target) return false;
    const y = target.getBoundingClientRect().top;
    return y >= 0 && y < innerHeight - 80;
  });
  checks.push('Authors can find the owner and land on the exact requirement heading');

  await page.goto(url('search?q=--param-file&audience=Users'), {waitUntil: 'networkidle'});
  await ready();
  assert.ok((await page.locator('.search-results').innerText()).includes('Invoke command reference'));
  checks.push('Users can find parameter reference alongside shared specifications');

  await page.goto(url('search?q=PR-REQ-0258&role=Tutorial'), {waitUntil: 'networkidle'});
  await ready();
  await page.getByText('No matches in this scope. Try fewer words or broaden the filters.').waitFor();
  await page.getByRole('button', {name: 'Search all documents'}).click();
  await page.locator('.search-results li').first().waitFor();
  assert.equal(await page.getByLabel('Search documentation', {exact: true}).inputValue(), 'PR-REQ-0258');
  checks.push('filtered-out results offer explicit widening without losing query');

  await page.getByLabel('Search documentation', {exact: true}).fill('restore');
  await openFilters();
  await page.getByLabel('Audience', {exact: true}).selectOption('Users');
  await page.waitForFunction(() => document.querySelector('.search-results')?.textContent.includes('Capture, restore, and migrate'));
  await page.screenshot({path: path.join(output, 'search-desktop.png'), fullPage: true});
  await page.reload({waitUntil: 'networkidle'});
  await ready();
  assert.equal(await page.getByLabel('Audience', {exact: true}).inputValue(), 'Users');
  assert.equal(await page.getByLabel('Search documentation', {exact: true}).inputValue(), 'restore');
  assert.ok((await page.locator('.filter-summary').innerText()).includes('Users'));
  checks.push('collapsed filters retain visible scope and URL state');
  await openFilters();
  await page.getByRole('button', {name: 'Reset filters', exact: true}).click();
  assert.equal(await page.getByLabel('Search documentation', {exact: true}).inputValue(), 'restore');
  const loader = await (await page.request.get(url('agent-docs/spec/interfaces/shell-loader.md'))).text();
  assert.ok(loader.includes('retained by default for Run inspection'));
  assert.ok(!loader.includes('Core discards its text'));
  checks.push('agent text preserves the diagnostic behavior contract');

  await page.goto(url('spec/persistence/persistence-schema-v11'), {waitUntil: 'networkidle'});
  await page.getByRole('heading', {name: 'Persistence Schema', exact: true}).waitFor();
  assert.ok(page.url().includes('/spec/storage/schema'));
  assert.ok(!(await page.locator('.theme-doc-markdown').innerText()).includes('Historical metadata-slice'));
  await page.screenshot({path: path.join(output, 'current-schema.png'), fullPage: false});
  const aliasedText = await (await page.request.get(url('agent-docs/spec/persistence/persistence-schema-v11.md'))).text();
  assert.ok(aliasedText.includes('Source: docs/spec/persistence/persistence-baseline.md'));
  checks.push('retired schema bookmarks resolve to the current schema in HTML and text');

  await page.goto(url('search?q=Vocabulary&role=Reference'), {waitUntil: 'networkidle'});
  await ready();
  await openFilters();
  assert.equal(await page.getByLabel('Document role').inputValue(), 'Reference');
  await page.getByRole('link', {name: 'Vocabulary', exact: true}).click();
  assert.ok(!(await page.locator('.doc-context').innerText()).includes('Informative'));
  assert.ok(!(await page.locator('.theme-doc-markdown').innerText()).includes('runtime remains deferred'));
  await page.screenshot({path: path.join(output, 'vocabulary.png'), fullPage: false});
  const glossaryText = await (await page.request.get(url('agent-docs/spec/core/vocabulary.md'))).text();
  assert.match(glossaryText, /^> Document context \(generated\): current \| Reference/);
  checks.push('vocabulary is a searchable reference without classification notices');

  for (const [version, role, anchor] of [[3, 'Instance', 'candidate-crate-private-repository-contract'], [4, 'Run', 'candidate-crate-private-repository-contract-1']]) {
    await page.goto(url(`pactrun-developers/architecture/persistence-schema-v${version}`), {waitUntil: 'networkidle'});
    await page.getByRole('link', {name: `${role} repository contract`, exact: true}).click();
    await page.locator('#' + anchor).waitFor();
    // Docusaurus appends a zero-width permalink glyph to rendered headings.
    const heading = (await page.locator('#' + anchor).innerText()).replace(/\u200b/g, '');
    assert.equal(heading, `${role} repository contract (crate-private)`);
    assert.equal(new URL(page.url()).hash, '#' + anchor);
  }
  checks.push('historical Instance and Run repository links land on distinct owning sections');

  await page.goto(url('spec/catalog'), {waitUntil: 'networkidle'});
  assert.equal(await page.locator('.theme-doc-markdown').getByRole('link', {name: 'Snapshot Integrity Format', exact: true}).count(), 1);
  const formatMap = await page.locator('.theme-doc-markdown').innerText();
  assert.ok(!formatMap.includes('V1 and V2 remain distinct supported'));
  assert.ok(!formatMap.includes('unresolved diagnostic presentation follow-up'));
  await page.screenshot({path: path.join(output, 'current-format-map.png'), fullPage: true});
  checks.push('format map presents the single current Snapshot owner and current diagnostics');

  await page.goto(url('search?q=nonexistent-zzyy-887766'), {waitUntil: 'networkidle'});
  await ready();
  await page.getByText('No matches in this scope. Try fewer words or broaden the filters.').waitFor();
  assert.equal(await page.getByRole('button', {name: 'Search all documents'}).count(), 0);
  checks.push('genuinely empty search is distinct from filtered-out results');

  await page.goto(url('search'), {waitUntil: 'networkidle'});
  await ready();
  await page.getByLabel('Search documentation', {exact: true}).focus();
  await page.keyboard.press('Tab');
  assert.equal(await page.locator(':focus').evaluate(el => el.tagName), 'SUMMARY');
  await page.keyboard.press('Enter');
  await page.keyboard.press('Tab');
  assert.equal(await page.locator(':focus').evaluate(el => el.tagName), 'SELECT');
  checks.push('keyboard opens filter disclosure and reaches its controls');

  await page.setViewportSize({width: 390, height: 844});
  await page.goto(url('search?q=restore'), {waitUntil: 'networkidle'});
  await ready();
  const resultY = await page.locator('.search-results li h2').first().evaluate(el => el.getBoundingClientRect().top);
  assert.ok(resultY < 764, 'first mobile result is below the useful viewport: ' + resultY);
  assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
  await page.screenshot({path: path.join(output, 'search-mobile.png'), fullPage: false});
  checks.push('mobile first result title is visible without opening filters');
  await page.goto(url('introduction'), {waitUntil: 'networkidle'});
  assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
  await page.screenshot({path: path.join(output, 'tutorial-mobile.png'), fullPage: false});
  checks.push('mobile tutorial has no viewport overflow');

  await page.route('**/document-catalog.json', route => route.abort());
  await page.goto(url('search'), {waitUntil: 'networkidle'});
  await page.getByRole('button', {name: 'Retry search'}).waitFor();
  await page.unroute('**/document-catalog.json');
  await page.getByRole('button', {name: 'Retry search'}).click();
  await ready();
  checks.push('failed index reports error and retries successfully');
  await page.emulateMedia({colorScheme: 'dark'});
  await page.goto(url('search?q=restore'), {waitUntil: 'networkidle'});
  await ready();
  assert.equal(await page.locator('html').getAttribute('data-theme'), 'dark');
  await page.screenshot({path: path.join(output, 'search-dark.png'), fullPage: false});
  checks.push('system dark preference remains readable');
  assert.deepEqual(errors, []);
  await writeFile(path.join(output, 'receipt.json'), JSON.stringify({status: 'Passed', base, checks, firstMobileResultY: resultY, pageErrors: errors}, null, 2));
  console.log(JSON.stringify({status: 'Passed', checks, firstMobileResultY: resultY}, null, 2));
} finally { await browser.close(); }

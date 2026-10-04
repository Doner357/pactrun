import assert from 'node:assert/strict';
import {mkdir, writeFile} from 'node:fs/promises';
import path from 'node:path';
import {pathToFileURL} from 'node:url';

const [base, output] = process.argv.slice(2);
if (!base || !output || !process.env.PLAYWRIGHT_PACKAGE) throw new Error('Provide base URL, evidence directory and PLAYWRIGHT_PACKAGE');
const {chromium} = await import(pathToFileURL(process.env.PLAYWRIGHT_PACKAGE).href);
await mkdir(output, {recursive: true});
const browser = await chromium.launch({headless: true});
const page = await browser.newPage({viewport: {width: 1440, height: 1000}});
const errors = [];
page.on('pageerror', error => errors.push(error.message));
const url = route => base.replace(/\/$/, '') + '/' + route;
const release = '1.0.0-alpha.4';
const checks = [];
const picker = () => page.locator('select[aria-label="Documentation version"]:visible');
const ready = () => page.waitForFunction(() => /matching documents/.test(document.querySelector('[role="status"]')?.textContent ?? ''));
try {
  await page.goto(url(''), {waitUntil: 'networkidle'});
  await page.waitForURL(url(release + '/'));
  assert.equal(await picker().inputValue(), release);
  checks.push('default entry selects the published release');

  await page.goto(url('spec/contracts/pack-source#pr-req-0258---packsourceyamlv1-schema-numbers-and-package-lineage'), {waitUntil: 'networkidle'});
  assert.ok(page.url().includes('/' + release + '/spec/contracts/pack-source#pr-req-0258'));
  await picker().selectOption('current');
  await page.waitForURL('**/next/spec/contracts/pack-source#pr-req-0258*');
  await page.locator('#pr-req-0258---packsourceyamlv1-schema-numbers-and-package-lineage').waitFor();
  await page.locator('.edition-bar').filter({hasText: 'Development (unreleased)'}).waitFor();
  const report = new URL(await page.getByRole('link', {name: 'Report a documentation issue'}).getAttribute('href'));
  assert.ok(report.searchParams.get('body').includes('Development (unreleased)'));
  assert.ok(report.searchParams.get('body').includes('/next/spec/contracts/pack-source'));
  checks.push('legacy anchors, same-topic version switching and issue context');

  await page.locator('a.navbar__brand').click();
  await page.waitForURL(url('next/'));
  await page.getByRole('link', {name: 'Commands', exact: true}).click();
  await page.waitForURL(url('next/commands'));
  await page.getByRole('heading', {name: 'Command help', exact: true}).waitFor();
  await picker().selectOption(release);
  await page.waitForURL(url(release + '/commands'));
  await page.getByText('Preserved from this release', {exact: false}).waitFor();
  await page.getByRole('link', {name: 'Invoke: parameters, timeouts, defaults, and examples'}).click();
  await page.waitForURL('**/' + release + '/pactrun-users/operations/invoke-reference');
  checks.push('brand, Commands and command-reference links preserve edition');

  await page.goto(url('next/guides/pack-management'), {waitUntil: 'networkidle'});
  await page.locator('.theme-doc-markdown').getByRole('link', {name: 'command overview'}).click();
  await page.waitForURL(url('next/commands'));
  checks.push('absolute Markdown command links remain in development');

  await page.goto(url('search?q=PR-REQ-0258&audience=Authors'), {waitUntil: 'networkidle'});
  await ready();
  assert.ok(page.url().includes('/' + release + '/search?'));
  let links = await page.locator('.search-results h2 a').evaluateAll(items => items.map(item => item.href));
  assert.ok(links.length > 0 && links.every(href => href.includes('/' + release + '/')));
  await picker().selectOption('current');
  await ready();
  await page.waitForFunction(() => [...document.querySelectorAll('.search-results h2 a')].every(a => a.href.includes('/next/')));
  assert.ok(page.url().includes('audience=Authors'));
  checks.push('search preserves query and filters, with edition-isolated results');

  await page.route('**/next/document-catalog.json', route => route.abort());
  await page.reload({waitUntil: 'networkidle'});
  await page.getByRole('button', {name: 'Retry search'}).waitFor();
  await page.unroute('**/next/document-catalog.json');
  await page.getByRole('button', {name: 'Retry search'}).click();
  await ready();
  checks.push('failed version index retries without falling back to another version');

  for (const [versionPath, id] of [[release, release], ['next', 'current']]) {
    const metadata = await (await page.request.get(url(versionPath + '/agent-docs/publication.json'))).json();
    assert.equal(metadata.documentation_version, id);
    const text = await (await page.request.get(url(versionPath + '/agent-docs/spec/index.md'))).text();
    assert.ok(text.includes('Documentation version: ' + (id === 'current' ? 'Development (unreleased)' : id)));
    await page.goto(url(versionPath + '/spec/'), {waitUntil: 'networkidle'});
    assert.equal(await page.locator('link[rel="describedby"]').getAttribute('href'), new URL(url(versionPath + '/llms.txt')).pathname);
  }
  checks.push('HTML discovery, text and metadata identify the same edition');

  await page.goto(url('next/not-a-real-page'), {waitUntil: 'networkidle'});
  await picker().selectOption(release);
  await page.getByRole('status').filter({hasText: 'not available'}).waitFor();
  checks.push('unavailable topic produces an explicit version-specific notice');

  await page.goto(url(release + '/spec/'), {waitUntil: 'networkidle'});
  await page.screenshot({path: path.join(output, 'version-desktop.png'), fullPage: false});
  await page.setViewportSize({width: 390, height: 844});
  await page.getByRole('button', {name: 'Toggle navigation bar'}).click();
  await picker().selectOption('current');
  await page.waitForURL(target => target.pathname.replace(/\/$/, '') === new URL(url('next/spec')).pathname);
  await page.waitForFunction(() => document.querySelector('.navbar__toggle')?.getAttribute('aria-expanded') === 'false');
  assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
  await page.screenshot({path: path.join(output, 'version-mobile.png'), fullPage: false});
  checks.push('mobile version selection is available and does not overflow');
  assert.deepEqual(errors, []);
  const receipt = {status: 'Passed', base, checks, pageErrors: errors};
  await writeFile(path.join(output, 'receipt.json'), JSON.stringify(receipt, null, 2));
  console.log(JSON.stringify(receipt, null, 2));
} catch (error) {
  await page.screenshot({path: path.join(output, 'failure.png'), fullPage: false});
  await writeFile(path.join(output, 'failure.json'), JSON.stringify({url: page.url(), checks, errors, message: String(error)}, null, 2));
  console.error('Failed at', page.url(), 'after', checks.length, 'checks; page errors:', errors);
  throw error;
} finally { await browser.close(); }

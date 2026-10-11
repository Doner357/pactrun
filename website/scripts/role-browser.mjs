import assert from 'node:assert/strict';
import {mkdir, writeFile} from 'node:fs/promises';
import {pathToFileURL} from 'node:url';
import path from 'node:path';
import {readerRole} from '../src/lib/reader-role.mjs';
const [base = 'http://127.0.0.1:3000/', output = 'role-browser-evidence'] = process.argv.slice(2);
const {chromium} = await import(pathToFileURL(process.env.PLAYWRIGHT_PACKAGE).href);
await mkdir(output, {recursive: true});
const browser = await chromium.launch({headless: true});
const page = await browser.newPage({viewport: {width: 1440, height: 1000}});
const checks = [], errors = [], visited = [];
const url = route => base.replace(/\/$/, '') + '/' + route;
page.on('pageerror', error => errors.push(error.message));
page.on('framenavigated', frame => {if (frame === page.mainFrame()) visited.push(new URL(frame.url()).pathname);});
const article = () => page.locator('.theme-doc-markdown');
async function open(route) {assert.equal((await page.goto(url(route), {waitUntil: 'networkidle'})).status(), 200);}
async function scopedSearch(role, query) {
  await page.getByRole('link', {name: role === 'Authors' ? 'Search author docs' : 'Search user docs', exact: true}).click();
  await page.waitForFunction(() => document.querySelector('[role="status"]')?.textContent.includes('matching documents'));
  assert.equal(await page.getByLabel('Audience', {exact: true}).inputValue(), role);
  await page.getByLabel('Search documentation', {exact: true}).fill(query);
  await page.waitForFunction(q => new URL(location.href).searchParams.get('q') === q, query);
  const link = page.locator('.search-results li h2 a').first();
  const href = await link.getAttribute('href');
  assert.equal(readerRole(new URL(href, page.url()).pathname, new URL(base).pathname), role);
  await link.click();
  await article().locator('h1').waitFor();
  assert.equal(readerRole(new URL(page.url()).pathname, new URL(base).pathname), role);
  checks.push(`${role} scoped search stays in reader guidance for ${query}`);
}
try {
  await open('package-authors/');
  await article().getByRole('link', {name: 'Prepare an isolated authoring workspace'}).click();
  await article().getByRole('heading', {name: 'Prepare an authoring workspace', exact: true}).waitFor();
  assert.ok((await article().innerText()).includes('PACTRUN_STORAGE_ROOT'));
  const nextTutorial = page.locator('.pagination-nav__link--next');
  assert.ok((await nextTutorial.innerText()).includes('Author your first Pack'));
  await nextTutorial.click();
  await article().getByRole('heading', {name: 'Author your first Pack', exact: true}).waitFor();
  assert.ok((await article().innerText()).includes('Windows variant'));
  checks.push('author landing, setup and executable tutorial stay in Authors');

  await open('package-authors/');
  await article().getByRole('link', {name: 'Pack fields, choices, defaults, and examples'}).click();
  await article().getByRole('heading', {name: 'Pack fields and values', exact: true}).waitFor();
  const fields = await article().innerText();
  for (const value of ['io.terminal', 'interactive', 'required', 'protection', 'parameters', 'There is no default terminal mode']) assert.ok(fields.includes(value), value);
  const sidebarLinks = await page.locator('.theme-doc-sidebar-container a[href]').evaluateAll(links => links.map(link => link.getAttribute('href')).filter(href => href.startsWith('/')));
  assert.ok(sidebarLinks.length > 5);
  assert.ok(sidebarLinks.every(href => readerRole(href, new URL(base).pathname) === 'Authors'));
  assert.ok(await page.locator('.theme-doc-sidebar-container').getByRole('link', {name: 'Pack fields and values', exact: true}).isVisible());
  await page.screenshot({path: path.join(output, 'author-fields-desktop.png'), fullPage: false});
  await scopedSearch('Authors', 'hook protocol');
  await open('package-authors/reference/pack-fields');
  await article().getByRole('link', {name: 'service fields', exact: true}).first().click();
  await article().getByRole('heading', {name: 'Service and Migration fields', exact: true}).waitFor();
  assert.ok((await article().innerText()).includes('reattach'));
  assert.ok((await article().innerText()).includes('user_mutation.kind'));
  checks.push('author can look up values, defaults and service-transition meanings locally');

  await open('package-authors/');
  await article().getByRole('link', {name: 'Direct Hook message reference'}).click();
  await article().getByRole('heading', {name: 'Direct Hook message reference', exact: true}).waitFor();
  assert.ok((await article().innerText()).includes('session_start'));
  assert.ok((await article().innerText()).includes('completion_accepted'));
  assert.equal(await article().locator('details').first().getAttribute('open'), null);
  assert.ok((await page.locator('.doc-context').innerText()).includes('REFERENCE'));
  checks.push('direct Hook author can read complete exchanges without opening Spec');

  await open('guides/');
  await article().getByRole('link', {name: 'User vocabulary', exact: true}).click();
  await article().getByRole('heading', {name: 'User vocabulary', exact: true}).waitFor();
  assert.ok((await article().innerText()).includes('Detached allocation'));
  await article().getByRole('link', {name: 'user reference', exact: true}).click();
  await article().getByRole('heading', {name: 'User reference', exact: true}).waitFor();
  const schemaUrl = await article().getByRole('link', {name: 'response schema', exact: true}).getAttribute('href');
  const response = await page.request.get(new URL(schemaUrl, page.url()).href);
  assert.ok(response.ok());
  const responseSchema = await response.json();
  assert.equal(responseSchema.properties.format.const, 'pactrun.cli');
  await article().getByRole('link', {name: 'JSON/JSONL reference', exact: true}).click();
  await article().getByRole('heading', {name: 'JSON and JSONL reference', exact: true}).waitFor();
  assert.ok((await article().innerText()).includes('format_version'));
  assert.ok((await article().innerText()).includes(responseSchema.properties.format_version.const));
  checks.push('user vocabulary, machine-output meanings and schema stay in Users');
  await scopedSearch('Users', 'snapshot integrity');

  await open('pactrun-users/reference/');
  await article().getByRole('link', {name: 'Retirement and recovery', exact: true}).click();
  await article().getByRole('heading', {name: 'Retirement and recovery details', exact: true}).waitFor();
  assert.ok((await article().innerText()).includes('--assert-cleanup-complete'));
  await open('guides/recovery');
  assert.ok((await article().innerText()).includes('pactrun instance resolve-manual-recovery'));
  checks.push('user can find recovery and deletion completion syntax without a developer page');

  await open('package-authors/managed-capabilities/snapshots-and-managed-data');
  // Section permalink controls contribute to the accessible heading name.
  const example = article().locator('h2').filter({hasText: 'Worked example: capture and read one value'});
  assert.ok(await example.isVisible());
  assert.equal(await example.evaluate(el => !!el.closest('details')), false);
  checks.push('worked Snapshot procedure is visible outside optional maintainer sources');

  for (const [route, role] of [['package-authors/reference/snapshot-limits', 'Authors'], ['pactrun-users/reference/snapshot-limits', 'Users']]) {
    await open(route);
    const links = await page.locator('.pagination-nav a').evaluateAll(items => items.map(item => item.getAttribute('href')));
    assert.ok(links.every(href => readerRole(href, new URL(base).pathname) === role));
  }
  checks.push('role sidebars and end-of-section pagination do not divert readers into other roles');

  await open('search?q=Product+and+Format+Compatibility');
  await page.waitForFunction(() => document.querySelector('[role="status"]')?.textContent.includes('matching documents'));
  assert.equal(await page.getByLabel('Audience', {exact: true}).inputValue(), '');
  assert.ok((await page.locator('.search-results li h2 a').first().getAttribute('href')).includes('/spec/storage/compatibility'));
  checks.push('unscoped exact-title lookup retains global relevance');

  await page.setViewportSize({width: 390, height: 844});
  await open('package-authors/reference/pack-fields');
  assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
  await page.screenshot({path: path.join(output, 'author-fields-mobile.png'), fullPage: false});
  checks.push('mobile field guide stays within viewport');
  await page.locator('.navbar__toggle').click();
  const mobileLinks = async () => {
    await page.locator('.navbar-sidebar__items--show-secondary').waitFor();
    await page.waitForFunction(() => {
      const panel = document.querySelector('.navbar-sidebar__item:not([inert])');
      if (!panel) return false;
      const bounds = panel.getBoundingClientRect();
      return Math.abs(bounds.left) < 1 && bounds.width > 0 && bounds.right <= innerWidth + 1;
    });
    // The off-screen main menu remains laid out but is inert, not reader navigation.
    return page.locator('.navbar-sidebar__item:not([inert]) a.menu__link:visible').evaluateAll(items => items.map(item => item.getAttribute('href')).filter(href => href?.startsWith('/')));
  };
  let mobile = await mobileLinks();
  assert.ok(mobile.length > 5);
  assert.ok(mobile.every(href => readerRole(href, new URL(base).pathname) === 'Authors'));
  await page.getByRole('button', {name: /Back to main menu/}).click();
  await page.locator('.navbar-sidebar').getByRole('link', {name: 'Users', exact: true}).click();
  await article().getByRole('heading', {name: 'User guides', exact: true}).waitFor();
  await page.locator('.navbar__toggle').click();
  mobile = await mobileLinks();
  assert.ok(mobile.length > 5);
  assert.ok(mobile.every(href => readerRole(href, new URL(base).pathname) === 'Users'));
  await page.screenshot({path: path.join(output, 'mobile-user-navigation.png'), fullPage: false, animations: 'disabled'});
  await page.locator('.navbar-sidebar__close').click();
  checks.push('mobile role switching replaces the author sidebar with user navigation');
  assert.ok(visited.every(route => !/\/(spec|development)(\/|$)/.test(route)), JSON.stringify(visited));
  assert.deepEqual(errors, []);
  await writeFile(path.join(output, 'receipt.json'), JSON.stringify({status: 'Passed', checks, visited, pageErrors: errors}, null, 2));
  console.log(JSON.stringify({status: 'Passed', checks}, null, 2));
} finally {await browser.close();}

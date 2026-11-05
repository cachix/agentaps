import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { webcrypto } from 'node:crypto';
import { beforeEach, test } from 'node:test';
import { runInNewContext } from 'node:vm';

// Import the exact browser module without making package settings part of the site.
const source = await readFile(new URL('../src/browser.js', import.meta.url), 'utf8');
const browser = await import(`data:text/javascript;base64,${Buffer.from(source).toString('base64')}`);
const html = await readFile(new URL('../index.html', import.meta.url), 'utf8');
const startupScript = html.match(/<script>([\s\S]*?)<\/script>/)[1];
const initializerSource = await readFile(new URL('../src/initializer.js', import.meta.url), 'utf8');
const { default: initializer } = await import(`data:text/javascript;base64,${Buffer.from(initializerSource).toString('base64')}`);

function startupPage() {
  const window = new EventTarget();
  const document = new EventTarget();
  const startup = { removed: false, remove() { this.removed = true; } };
  const status = { textContent: 'Loading Remote Control…' };
  const bar = { hidden: false, value: undefined, removeAttribute() { this.value = undefined; } };
  const retry = Object.assign(new EventTarget(), { hidden: true });
  document.getElementById = id => ({ startup, 'startup-status': status, 'startup-progress': bar, 'startup-retry': retry })[id];
  let timeout;
  let reloads = 0;
  runInNewContext(startupScript, {
    window, document,
    location: { reload() { reloads += 1; } },
    setTimeout(callback) { timeout = callback; },
  });
  return { window, document, startup, status, bar, retry, tick: () => timeout(), reloads: () => reloads };
}

test('download progress follows real byte counts and waits for the app to become ready', () => {
  const page = startupPage();
  globalThis.window = page.window;
  const loader = initializer();
  loader.onStart();
  assert.equal(page.bar.value, undefined);
  loader.onProgress({ current: 250, total: 1000 });
  assert.equal(page.bar.value, 25);
  assert.match(page.status.textContent, /25%/);
  loader.onProgress({ current: 1000, total: 1000 });
  assert.equal(page.bar.value, 100);
  assert.match(page.status.textContent, /Starting/);
  loader.onSuccess();
  assert.equal(page.bar.value, undefined);
  assert.equal(page.startup.removed, false);
  page.window.dispatchEvent(new Event('AgentapsReady'));
  assert.equal(page.startup.removed, true);
  loader.onSuccess();
  loader.onFailure();
  assert.equal(page.retry.hidden, true);
});

test('downloads without a known size stay indeterminate and report received bytes', () => {
  const page = startupPage();
  globalThis.window = page.window;
  const loader = initializer();
  loader.onProgress({ current: 3145728, total: 0 });
  assert.equal(page.bar.value, undefined);
  assert.match(page.status.textContent, /3\.0 MB/);
  assert(!page.status.textContent.includes('%'));
  loader.onProgress({ current: 1200, total: 1000 });
  assert.equal(page.bar.value, 100);
});

test('failed initialization stops the loading bar and cannot be overwritten by progress', () => {
  const page = startupPage();
  globalThis.window = page.window;
  const loader = initializer();
  loader.onFailure(new Error('test error'));
  assert.equal(page.bar.hidden, true);
  assert.equal(page.retry.hidden, false);
  loader.onProgress({ current: 50, total: 100 });
  loader.onSuccess();
  assert.match(page.status.textContent, /could not start/);
});

test('startup errors before the body loads show a retry without exposing error details', () => {
  const page = startupPage();
  const getElementById = page.document.getElementById;
  page.document.getElementById = () => null;
  page.window.dispatchEvent(new Event('error'));
  page.document.getElementById = getElementById;
  page.document.dispatchEvent(new Event('DOMContentLoaded'));
  assert.match(page.status.textContent, /could not start/);
  assert.equal(page.retry.hidden, false);
  page.tick();
  assert.match(page.status.textContent, /could not start/);
  page.retry.dispatchEvent(new Event('click'));
  assert.equal(page.reloads(), 1);
});

test('rejected startup shows failure and slow loading stays visible until the app is ready', () => {
  const failed = startupPage();
  failed.window.dispatchEvent(new Event('unhandledrejection'));
  assert.match(failed.status.textContent, /could not start/);
  const page = startupPage();
  page.tick();
  assert.match(page.status.textContent, /Still loading/);
  assert.equal(page.retry.hidden, false);
  page.window.dispatchEvent(new Event('AgentapsReady'));
  assert.equal(page.startup.removed, true);
  const status = page.status.textContent;
  page.window.dispatchEvent(new Event('error'));
  assert.equal(page.status.textContent, status);
});

const records = new Map();
const storageKey = 'agentaps.connection.v1';
const pairing = `${'a'.repeat(64)}:${'b'.repeat(64)}`;
const passphrase = 'a long test passphrase';
const credential = {
  rawId: Uint8Array.of(1, 2, 3).buffer,
  getClientExtensionResults: () => ({ prf: { results: { first: new Uint8Array(32).fill(7).buffer } } }),
};

beforeEach(() => {
  records.clear();
  browser.discardPasskeyEnrollment();
  Object.defineProperty(globalThis, 'crypto', { value: webcrypto, configurable: true });
  globalThis.localStorage = {
    get length() { return records.size; },
    key: index => [...records.keys()][index],
    getItem: key => records.get(key) ?? null,
    setItem: (key, value) => records.set(key, value),
  };
  Object.defineProperty(globalThis, 'navigator', { value: {
    credentials: { create: async () => credential, get: async () => credential },
  }, configurable: true });
  globalThis.isSecureContext = true;
  globalThis.PublicKeyCredential = class {};
});

test('passphrase saves an encrypted record and restores the pairing', async () => {
  const id = await browser.saveConnection(passphrase, pairing);
  const record = records.get(`${storageKey}.${id}`);
  assert(!record.includes(pairing));
  assert(!record.includes(passphrase));
  assert.equal(await browser.unlockConnection(passphrase, id), pairing);
  assert.equal(JSON.parse(browser.savedConnections())[0].protection, 1);
  await assert.rejects(browser.unlockConnection('another long passphrase', id), /Could not unlock/);
});

test('short passphrases never write a record', async () => {
  await assert.rejects(browser.saveConnection('too short', pairing), /15 characters/);
  assert.equal(records.size, 0);
});

test('encrypted records cannot be moved to another desktop ID', async () => {
  const id = await browser.saveConnection(passphrase, pairing);
  const other = 'c'.repeat(64);
  records.set(`${storageKey}.${other}`, records.get(`${storageKey}.${id}`));
  await assert.rejects(browser.unlockConnection(passphrase, other), /Could not unlock/);
});

test('damaged storage and invalid IDs produce actionable errors', async () => {
  const id = 'd'.repeat(64);
  records.set(`${storageKey}.${id}`, '{broken');
  assert.deepEqual(JSON.parse(browser.savedConnections()), []);
  await assert.rejects(browser.unlockConnection(passphrase, id), /damaged/);
  await assert.rejects(browser.unlockConnection(passphrase, '../invalid'), /Invalid saved connection/);
});

test('phone unlock encrypts and restores the pairing with WebAuthn PRF', async () => {
  await browser.beginPasskeyEnrollment();
  const id = await browser.finishPasskeyEnrollment(pairing);
  assert.equal(JSON.parse(browser.savedConnections())[0].protection, 2);
  assert(!records.get(`${storageKey}.${id}`).includes(pairing));
  assert.equal(await browser.beginPasskeyUnlock(id), pairing);
  await assert.rejects(browser.unlockConnection(passphrase, id), /phone unlock/);
});

test('cancelled phone enrollment cannot save a connection', async () => {
  await browser.beginPasskeyEnrollment();
  browser.discardPasskeyEnrollment();
  await assert.rejects(browser.finishPasskeyEnrollment(pairing), /interrupted/);
  assert.equal(records.size, 0);
});

test('phone unlock rejects credentials without PRF support', async () => {
  navigator.credentials.create = async () => ({ ...credential, getClientExtensionResults: () => ({}) });
  navigator.credentials.get = async () => ({ ...credential, getClientExtensionResults: () => ({}) });
  await assert.rejects(browser.beginPasskeyEnrollment(), /does not support secure phone unlock/);
  assert.equal(records.size, 0);
});

import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { webcrypto } from 'node:crypto';
import { beforeEach, test } from 'node:test';

// Import the exact browser module without making package settings part of the site.
const source = await readFile(new URL('../src/browser.js', import.meta.url), 'utf8');
const browser = await import(`data:text/javascript;base64,${Buffer.from(source).toString('base64')}`);
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

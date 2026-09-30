const storageKey = 'agentaps.connection.v1';
const iterations = 600000;
const encoder = new TextEncoder();
const decoder = new TextDecoder();
let pendingPasskey = null;
let qrCamera = null;

export function browserClientName() {
  const agent = navigator.userAgent;
  if (/iPad/i.test(agent)) return 'iPad browser';
  if (/iPhone|iPod/i.test(agent)) return 'iPhone browser';
  if (/Android/i.test(agent)) return 'Android browser';
  return 'Desktop browser';
}

function stopCamera(session) {
  session.cancelled = true;
  session.stream?.getTracks().forEach(track => track.stop());
  document.removeEventListener('visibilitychange', session.onVisibilityChange);
  session.overlay.remove();
  if (qrCamera === session) qrCamera = null;
}
export function stopQrCamera() {
  if (qrCamera) stopCamera(qrCamera);
}
export async function startQrCamera() {
  if (!globalThis.isSecureContext || !navigator.mediaDevices?.getUserMedia) {
    throw new Error('Camera access needs HTTPS or localhost in a browser that supports it.');
  }
  stopQrCamera();
  const overlay = document.createElement('div');
  overlay.style.cssText = 'position:fixed;inset:0;z-index:2147483647;background:#10151c;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:16px;padding:24px;color:#edf2f7;font:16px sans-serif;text-align:center';
  const title = document.createElement('div');
  title.textContent = 'Scan the QR code shown in desktop Agentaps';
  const video = document.createElement('video');
  video.autoplay = true;
  video.muted = true;
  video.playsInline = true;
  video.style.cssText = 'width:min(100%,480px);max-height:70vh;object-fit:contain;border-radius:12px;background:#202a36';
  const cancel = document.createElement('button');
  cancel.textContent = 'Cancel';
  cancel.style.cssText = 'border:0;border-radius:8px;background:#304b60;color:#edf2f7;padding:12px 24px;font:inherit;cursor:pointer';
  overlay.append(title, video, cancel);
  document.body.append(overlay);
  const canvas = document.createElement('canvas');
  const context = canvas.getContext('2d', { willReadFrequently: true });
  if (!context) {
    overlay.remove();
    throw new Error('Could not read camera frames');
  }
  const session = { overlay, video, canvas, context, stream: null, cancelled: false };
  session.onVisibilityChange = () => { if (document.hidden) stopCamera(session); };
  document.addEventListener('visibilitychange', session.onVisibilityChange);
  qrCamera = session;
  cancel.addEventListener('click', () => stopCamera(session));
  try {
    const stream = await navigator.mediaDevices.getUserMedia({ audio: false, video: { facingMode: { ideal: 'environment' } } });
    if (session.cancelled) {
      stream.getTracks().forEach(track => track.stop());
      throw new Error('Camera scan cancelled');
    }
    session.stream = stream;
    video.srcObject = stream;
    await video.play();
    if (session.cancelled) throw new Error('Camera scan cancelled');
  } catch (error) {
    stopCamera(session);
    throw error;
  }
}
export async function nextQrFrame() {
  await new Promise(resolve => setTimeout(resolve, 180));
  const session = qrCamera;
  if (!session || session.cancelled) throw new Error('Camera scan cancelled');
  const { video } = session;
  if (session.stream?.getVideoTracks()[0]?.readyState === 'ended') {
    throw new Error('Camera disconnected. Try scanning again.');
  }
  if (!video.videoWidth || !video.videoHeight) return null;
  const width = Math.min(640, video.videoWidth);
  const height = Math.round(video.videoHeight * width / video.videoWidth);
  session.canvas.width = width;
  session.canvas.height = height;
  session.context.drawImage(video, 0, 0, width, height);
  const rgba = session.context.getImageData(0, 0, width, height).data;
  const pixels = new Uint8Array(width * height);
  for (let i = 0, j = 0; i < pixels.length; i++, j += 4) {
    pixels[i] = (rgba[j] * 77 + rgba[j + 1] * 150 + rgba[j + 2] * 29) >> 8;
  }
  return { width, height, pixels };
}

function bytesToHex(bytes) {
  return Array.from(bytes, byte => byte.toString(16).padStart(2, '0')).join('');
}
function hexToBytes(hex) {
  if (!/^(?:[0-9a-f]{2})+$/.test(hex)) throw new Error('Saved connection is damaged');
  return Uint8Array.from(hex.match(/../g), pair => parseInt(pair, 16));
}
async function keyFromPassphrase(passphrase, salt) {
  const material = await crypto.subtle.importKey('raw', encoder.encode(passphrase), 'PBKDF2', false, ['deriveKey']);
  return crypto.subtle.deriveKey(
    { name: 'PBKDF2', hash: 'SHA-256', salt, iterations },
    material, { name: 'AES-GCM', length: 256 }, false, ['encrypt', 'decrypt']
  );
}
async function keyFromPrf(output, salt) {
  const material = await crypto.subtle.importKey('raw', output, 'HKDF', false, ['deriveKey']);
  return crypto.subtle.deriveKey(
    { name: 'HKDF', hash: 'SHA-256', salt, info: encoder.encode('Agentaps phone unlock v2') },
    material, { name: 'AES-GCM', length: 256 }, false, ['encrypt', 'decrypt']
  );
}
function connectionKey(id) {
  if (id === 'default') return storageKey;
  if (!/^[0-9a-f]{64}$/.test(id)) throw new Error('Invalid saved connection');
  return `${storageKey}.${id}`;
}
function readConnection(id) {
  const raw = localStorage.getItem(connectionKey(id));
  if (!raw) throw new Error('No saved connection. Pair with the desktop again.');
  try { return JSON.parse(raw); } catch (_) { throw new Error('Saved connection is damaged'); }
}
async function connectionId(pairing) {
  const endpoint = pairing.split(':', 1)[0];
  const hash = await crypto.subtle.digest('SHA-256', encoder.encode(endpoint));
  return bytesToHex(new Uint8Array(hash));
}
export function savedConnections() {
  const saved = [];
  for (let index = 0; index < localStorage.length; index++) {
    const key = localStorage.key(index);
    if (key !== storageKey && !key?.startsWith(`${storageKey}.`)) continue;
    const id = key === storageKey ? 'default' : key.slice(storageKey.length + 1);
    if (id !== 'default' && !/^[0-9a-f]{64}$/.test(id)) continue;
    try {
      const record = JSON.parse(localStorage.getItem(key));
      if (record.version !== 1 && record.version !== 2) continue;
      saved.push({
        id,
        label: typeof record.label === 'string' ? record.label : 'Saved desktop',
        protection: record.version,
      });
    } catch (_) { /* A damaged record cannot be unlocked. */ }
  }
  saved.sort((a, b) => a.label.localeCompare(b.label));
  return JSON.stringify(saved);
}
export function renameSavedConnection(id) {
  const record = readConnection(id);
  const name = globalThis.prompt('Desktop name', record.label || 'Saved desktop');
  if (name === null) return false;
  const label = name.trim();
  if (!label || Array.from(label).length > 64) return false;
  record.label = label;
  localStorage.setItem(connectionKey(id), JSON.stringify(record));
  return true;
}
export function promptProjectPath() {
  return globalThis.prompt('Project path or ssh://host/absolute/path')?.trim() || null;
}
export function promptAcpCommand() {
  return globalThis.prompt('ACP command and arguments')?.trim() || null;
}
function connectionLabel(keyName, endpoint) {
  try {
    const label = JSON.parse(localStorage.getItem(keyName))?.label;
    if (typeof label === 'string' && label.trim()) return label;
  } catch (_) { /* A damaged record will be replaced when pairing again. */ }
  return `Desktop ${endpoint.slice(0, 8)}`;
}
export function canUsePhoneUnlock() {
  return !!(globalThis.isSecureContext && globalThis.PublicKeyCredential && navigator.credentials?.create && navigator.credentials?.get);
}
export async function saveConnection(passphrase, pairing) {
  if (Array.from(passphrase).length < 15) throw new Error('Use at least 15 characters for the unlock passphrase');
  const id = await connectionId(pairing);
  const keyName = connectionKey(id);
  const endpoint = pairing.split(':', 1)[0];
  const salt = crypto.getRandomValues(new Uint8Array(16));
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const key = await keyFromPassphrase(passphrase, salt);
  const ciphertext = await crypto.subtle.encrypt(
    { name: 'AES-GCM', iv, additionalData: encoder.encode(keyName) },
    key, encoder.encode(pairing)
  );
  localStorage.setItem(keyName, JSON.stringify({
    version: 1, label: connectionLabel(keyName, endpoint),
    salt: bytesToHex(salt), iv: bytesToHex(iv), ciphertext: bytesToHex(new Uint8Array(ciphertext))
  }));
  return id;
}
export async function unlockConnection(passphrase, id) {
  const record = readConnection(id);
  if (record.version !== 1) throw new Error('This connection uses phone unlock');
  const salt = hexToBytes(record.salt);
  const iv = hexToBytes(record.iv);
  const ciphertext = hexToBytes(record.ciphertext);
  if (salt.length !== 16 || iv.length !== 12) throw new Error('Saved connection is damaged');
  try {
    const key = await keyFromPassphrase(passphrase, salt);
    return decoder.decode(await crypto.subtle.decrypt(
      { name: 'AES-GCM', iv, additionalData: encoder.encode(connectionKey(id)) }, key, ciphertext
    ));
  } catch (_) {
    throw new Error('Could not unlock. Check your passphrase.');
  }
}
function base64url(bytes) {
  return btoa(String.fromCharCode(...bytes)).replaceAll('+', '-').replaceAll('/', '_').replace(/=+$/, '');
}
function prfOutput(credential) {
  const output = credential?.getClientExtensionResults()?.prf?.results?.first;
  if (!output || output.byteLength !== 32) {
    throw new Error('This phone or passkey does not support secure phone unlock. Use a passphrase instead.');
  }
  return output;
}
function assertionOptions(id, prfSalt) {
  const credentialId = hexToBytes(id);
  if (!credentialId.length || credentialId.length > 1024) throw new Error('Saved connection is damaged');
  return {
    challenge: crypto.getRandomValues(new Uint8Array(32)),
    allowCredentials: [{ type: 'public-key', id: credentialId }],
    userVerification: 'required',
    extensions: { prf: { evalByCredential: { [base64url(credentialId)]: { first: prfSalt } } } },
  };
}
export async function beginPasskeyEnrollment() {
  if (!canUsePhoneUnlock()) throw new Error('Phone unlock needs a compatible browser on HTTPS. Use a passphrase instead.');
  const prfSalt = crypto.getRandomValues(new Uint8Array(32));
  const creation = navigator.credentials.create({ publicKey: {
    challenge: crypto.getRandomValues(new Uint8Array(32)),
    rp: { name: 'Agentaps' },
    user: {
      id: crypto.getRandomValues(new Uint8Array(16)),
      name: 'Agentaps on this phone', displayName: 'Agentaps on this phone',
    },
    pubKeyCredParams: [{ type: 'public-key', alg: -7 }],
    timeout: 60000,
    authenticatorSelection: {
      authenticatorAttachment: 'platform', residentKey: 'required', userVerification: 'required',
    },
    extensions: { prf: { eval: { first: prfSalt } } },
  }});
  const credential = await creation;
  if (!credential) throw new Error('Phone unlock was cancelled');
  const id = bytesToHex(new Uint8Array(credential.rawId));
  let output = credential.getClientExtensionResults()?.prf?.results?.first;
  if (!output) {
    const assertion = await navigator.credentials.get({ publicKey: assertionOptions(id, prfSalt) });
    output = prfOutput(assertion);
  }
  if (output.byteLength !== 32) throw new Error('This phone or passkey does not support secure phone unlock. Use a passphrase instead.');
  const kdfSalt = crypto.getRandomValues(new Uint8Array(16));
  pendingPasskey = { id, prfSalt: bytesToHex(prfSalt), kdfSalt: bytesToHex(kdfSalt), key: await keyFromPrf(output, kdfSalt) };
}
export async function finishPasskeyEnrollment(pairing) {
  if (!pendingPasskey) throw new Error('Phone unlock setup was interrupted');
  const id = await connectionId(pairing);
  const keyName = connectionKey(id);
  const endpoint = pairing.split(':', 1)[0];
  const prepared = pendingPasskey;
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const ciphertext = await crypto.subtle.encrypt(
    { name: 'AES-GCM', iv, additionalData: encoder.encode(keyName) },
    prepared.key, encoder.encode(pairing)
  );
  localStorage.setItem(keyName, JSON.stringify({
    version: 2, label: connectionLabel(keyName, endpoint),
    credentialId: prepared.id, prfSalt: prepared.prfSalt,
    kdfSalt: prepared.kdfSalt, iv: bytesToHex(iv),
    ciphertext: bytesToHex(new Uint8Array(ciphertext)),
  }));
  pendingPasskey = null;
  return id;
}
export function discardPasskeyEnrollment() {
  pendingPasskey = null;
}
export async function beginPasskeyUnlock(id) {
  if (!canUsePhoneUnlock()) throw new Error('Phone unlock is unavailable in this browser');
  const record = readConnection(id);
  if (record.version !== 2) throw new Error('This connection uses a passphrase');
  const prfSalt = hexToBytes(record.prfSalt);
  const kdfSalt = hexToBytes(record.kdfSalt);
  const iv = hexToBytes(record.iv);
  const ciphertext = hexToBytes(record.ciphertext);
  if (prfSalt.length !== 32 || kdfSalt.length !== 16 || iv.length !== 12) throw new Error('Saved connection is damaged');
  const assertion = await navigator.credentials.get({ publicKey: assertionOptions(record.credentialId, prfSalt) });
  if (!assertion || bytesToHex(new Uint8Array(assertion.rawId)) !== record.credentialId) {
    throw new Error('Wrong phone unlock credential');
  }
  const key = await keyFromPrf(prfOutput(assertion), kdfSalt);
  try {
    return decoder.decode(await crypto.subtle.decrypt(
      { name: 'AES-GCM', iv, additionalData: encoder.encode(connectionKey(id)) }, key, ciphertext
    ));
  } catch (_) { throw new Error('Could not unlock saved connection'); }
}
export function clearPairingHash() {
  history.replaceState(null, '', location.pathname + location.search);
}
export function pageHidden() {
  return document.hidden;
}

export function pollDelay() { return new Promise(resolve => setTimeout(resolve, 800)); }

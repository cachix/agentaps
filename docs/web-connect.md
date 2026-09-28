# Web Connect

Web Connect is a browser preview for Agentaps desktop sessions. Its static site serves the landing page at `/` and the browser UI at `/connect/`. The browser connects directly to the running desktop app through Iroh. Agent processes and project files stay on the desktop computer.

## Pair a browser

1. Open Agentaps on the desktop and select the phone icon next to Archive. A QR code and linked browsers appear in the main window.
2. On your phone, open [Web Connect](https://agentaps.dev/connect/) and allow camera access to scan the QR code. You can also open the pairing link directly. The site in the QR code must match the site open on your phone.
3. Choose **Pair with phone unlock** or **Pair with passphrase**. Phone unlock requires a compatible browser and passkey with WebAuthn PRF support; the system may verify you with a fingerprint, face scan, or device PIN. Otherwise, use a unique passphrase of at least 15 characters.

The pairing link contains a one time enrollment secret. Keep unused links private. When you open a link, the page removes the secret from the address bar; scanning within the page keeps it out of the address bar altogether. The page exchanges the secret for an access token and stores that token and the desktop's public Iroh ID as an AES-GCM encrypted browser record. The pairing link then expires. Pair on the final HTTPS site because browser storage and passkeys belong to that site's origin.

The desktop stores its Iroh identity and linked browser credentials in your user-global SecretSpec provider. Configure a default with `secretspec config global init`. If the default is missing or fails, Agentaps shows the path it checked and lets you select a provider for this run. Choices include a system keyring, 1Password, or another SecretSpec provider name or URI. This choice does not change the default. Select the same provider next time to preserve existing pairings. Text based providers can store the UTF-8 JSON value.

## Reconnect and manage access

Return to [Web Connect](https://agentaps.dev/connect/) to see desktops saved in this browser. You can rename a saved desktop, unlock it with your chosen method, or pair another one. The page locks when hidden or reloaded. If browser storage is cleared, the passkey becomes unavailable, or you forget the passphrase, pair again from the desktop. To change protection methods, open a new pairing link and choose the other method.

The desktop pairing view lists linked browsers. Revoke a browser there to prevent new requests from it. Older shared-token pairings appear as **Previously paired browsers** and can only be revoked together. A revoked browser may still show cached conversation content until it refreshes. Pair it again with a fresh QR code if needed.

The unlock method protects the saved token if someone gets your phone. It does not protect an already open browser session or a phone whose passphrase or device PIN is known to the person holding it. Closing Agentaps ends browser access until the app runs again. To rotate the Iroh identity and revoke every browser and pairing link, close Agentaps and remove the `MOBILE_CREDENTIALS` entry for project `agentaps` and profile `default` from the provider you used, then start Agentaps again.

## Browser capabilities and limits

The browser can read active conversations, start a local or SSH session, send prompts, stop turns, and answer ACP permission requests. **Other project** accepts a local absolute path or `ssh://host/absolute/path`; **Custom ACP command** accepts an executable and arguments. It shows the latest 100 messages per session and shortens long messages. Diff review and ACP form questions are not yet available in the browser.

Both devices need network access to a compatible Iroh relay. The browser uses WebGPU when available and falls back to WebGL2 if WebGPU cannot initialize.

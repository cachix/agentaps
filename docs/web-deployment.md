# Web deployment

The static website and Web Connect UI live in `web/`. The landing page is at `/`, and Web Connect is at `/connect/`.

The development environment includes the `wasm32-unknown-unknown` Rust target, Trunk, and Clang. Build the site with:

```sh
devenv shell -- bash web/build.sh
```

The build output is `web/dist/`. To test it locally on the same computer as Agentaps:

```sh
python3 -m http.server 8080 --directory web/dist
```

Set `AGENTAPS_WEB_URL=http://localhost:8080/` when starting the desktop app, or set Web Connect address in Settings > Advanced, so its pairing link points to the local site. This local URL is for testing in a browser on the same computer. For phone testing, serve the site at an address the phone can reach and set `AGENTAPS_WEB_URL` to that address. Phone unlock requires HTTPS. By default, pairing links point to `https://agentaps.dev/`. The landing page forwards pairing links to `/connect/` without sending the secret to the server. Use the same site origin on the phone as the one in the desktop QR code. Browser storage and passkeys are tied to that origin.

## Cloudflare Pages

The `agentaps-site` Cloudflare Pages project builds and publishes GitHub `main` with `web/cloudflare-build.sh`. Its build output is `web/dist/`. The Cloudflare Workers & Pages GitHub app has access to `domenkozar/agentaps`, so pushes to `main` trigger production deployments. Build results appear in the project's Deployments tab.

With Wrangler signed in to the same Cloudflare account, you can build and publish manually:

```sh
devenv shell -- bash web/deploy.sh
```

Attach `agentaps.dev` as a Cloudflare Pages custom domain. Its Cloudflare DNS zone needs a proxied CNAME record named `@` pointing to `agentaps-site.pages.dev`. Wait for Cloudflare to mark the domain active, then check that both `https://agentaps.dev/` and `https://agentaps.dev/connect/` load before sharing pairing links.

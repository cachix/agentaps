# Web deployment

The website and Web Connect UI live in `web/`. The website is an Astro and Starlight site in `web/site/` built on the shared [Cachix site kit](https://github.com/cachix/site-kit). The landing page is at `/`, and Web Connect is at `/connect/`.

To work on the website with live reload:

```sh
devenv shell -- npm --prefix web/site install
devenv shell -- npm --prefix web/site run dev
```

The development environment includes Node, the `wasm32-unknown-unknown` Rust target, Trunk, and Clang. Build the website and Web Connect with:

```sh
devenv shell -- bash web/build.sh
```

The build output is `web/dist/`. Check its links with `devenv shell -- npm --prefix web/site run check:links`. To test it locally on the same computer as Agentaps:

```sh
python3 -m http.server 8080 --directory web/dist
```

Set `AGENTAPS_WEB_URL=http://localhost:8080/` when starting the desktop app, or set Web Connect address in Settings > Advanced, so its pairing link points to the local site. This local URL is for testing in a browser on the same computer. For phone testing, serve the site at an address the phone can reach and set `AGENTAPS_WEB_URL` to that address. Phone unlock requires HTTPS. By default, pairing links point to `https://agentaps.dev/`. The landing page forwards pairing links to `/connect/` without sending the secret to the server. Use the same site origin on the phone as the one in the desktop QR code. Browser storage and passkeys are tied to that origin.

## Cloudflare Workers

Connect the `cachix/agentaps` GitHub repository to a Cloudflare Worker with Workers Builds and these settings:

| Setting | Value |
| --- | --- |
| Production branch | `main` |
| Root directory | `/` |
| Build command | `bash web/cloudflare-build.sh` |
| Deploy command | `npx wrangler deploy --config web/site/wrangler.jsonc` |

Pushes to `main` then publish the site. [`web/site/wrangler.jsonc`](../web/site/wrangler.jsonc) serves `web/dist/` as static assets through [`web/site/worker.js`](../web/site/worker.js). The Worker serves GitHub stars and the latest release at `/api/github` for the navigation bar, and answers Markdown requests from coding agents through the site kit middleware. An optional `GITHUB_TOKEN` secret raises the GitHub API rate limit.

If deployment reports an oversized `web/target/.../agentaps_web.wasm`, check that
the deploy command uses the Wrangler config above. Pointing `--assets` at `web/`
uploads Rust's intermediate build files along with the website. The public assets
directory is `web/dist/`; Trunk processes the raw WASM into the browser bundle
under `web/dist/connect/`. Release builds use Trunk's `data-wasm-opt="z"` setting
to optimize that bundle for size.

With Wrangler signed in to the same Cloudflare account, you can build and publish manually:

```sh
devenv shell -- bash web/deploy.sh
```

After the Worker deploys, move `agentaps.dev` from the `agentaps-site` Pages project to the Worker under **Settings > Domains & Routes > Add > Custom Domain**. Until then, the Pages project keeps publishing the same build without the Worker, so the navigation bar shows no GitHub stars or release version. Check that both `https://agentaps.dev/` and `https://agentaps.dev/connect/` load before sharing pairing links.

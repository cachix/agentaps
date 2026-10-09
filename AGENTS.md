# Agent instructions

- Update `CHANGELOG.md` with every repository change. Add a concise entry under `Unreleased` in the same change, including for documentation, fixes, features, and dependency updates. Describe the effect rather than the implementation steps.
- When preparing a release, move its entries from `Unreleased` to a version heading with the release date, then leave `Unreleased` ready for future entries.
- Follow `docs/releasing.md` for releases. Publishing a stable GitHub release triggers crates.io publication from its tag; the tag must match the desktop Cargo version. Bump the protocol crate version and its desktop dependency requirement whenever publishing protocol changes.
- Keep automated crates.io publication on trusted publishing. If the repository owner, publishing workflow filename, or job environment changes, update the trusted publisher configuration for both crates and verify it with an authenticated dry run before releasing.
- Before preparing or submitting a contribution, read and follow the repository's applicable contribution guidelines, local instructions, and PR template. Check requirements for testing, formatting, commit messages, target branches, review, and automation or AI disclosure.
- When finishing changes, summarize what you would do next. You may state a preference and other options.
- Do not use em dashes.

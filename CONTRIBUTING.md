# Contributing to Gantry

Thanks for your interest in improving Gantry. This guide gets you from clone to pull
request.

## Ways to contribute

- **Report bugs** — open a [bug report](https://github.com/trygantry/gantry/issues/new/choose).
- **Suggest features** — open a feature request. Keep Gantry focused on launch
  readiness, not general note-taking.
- **Improve docs** — README, schema docs, examples.
- **Write code** — look for issues labeled `good first issue` or `help wanted`.

## Development setup

Prerequisites:

- Rust (stable) — <https://rustup.rs>
- Node.js 18+
- Tauri system dependencies — <https://tauri.app/start/prerequisites/>

```bash
git clone https://github.com/trygantry/gantry.git
cd gantry
npm install
npm run tauri dev      # run the desktop app
```

Build and try the CLI:

```bash
cargo build -p gantry-cli
cargo run -p gantry-cli -- --help
```

## Before you open a pull request

Run the same checks CI runs:

```bash
cargo fmt --all                            # format
cargo check -p gantry-core -p gantry-cli   # compile GUI-free CLI + core (no system deps)
cargo test  -p gantry-core -p gantry-cli   # tests
npm run test:frontend                      # frontend syntax and contract tests
```

Then:

1. Keep the change focused; one logical change per PR.
2. Add or update unit tests for any new or modified backend behavior.
3. Update docs when behavior changes.
4. Fill out the PR template and link any related issue.

## Architecture at a glance

| Module | Responsibility |
| :--- | :--- |
| Vault Manager | Open, initialize, validate `.gantry/` vaults |
| File Watcher | Detect external edits from IDEs and AI agents |
| Parser | Parse markdown, frontmatter, checklists, wiki links |
| Validator | Validate schemas, required docs, links, freshness |
| Indexer | Generate `.gantry/index.json` |
| Readiness Engine | Compute Go / At Risk / No-Go |
| CLI | Validation, indexing, summaries, audit for agents and CI |
| Desktop UI | Render human views and write safe file updates |

Core logic lives in `crates/gantry-core/src/lib.rs`; the CLI entrypoint is
`crates/gantry-cli/src/main.rs`; the Tauri desktop shim is `src-tauri/src/`; the frontend is in `src/`.

## File write safety

Gantry must never corrupt user-authored markdown. When touching write paths:

- Only rewrite the smallest necessary section.
- Preserve the markdown body below the frontmatter `---` delimiter.
- Treat `.gantry/index.json` as generated and safe to rewrite.

## Commit and PR conventions

- Write clear commit messages describing the *why*.
- By contributing, you agree your contributions are licensed under
  [Apache-2.0](./LICENSE).

## Code of Conduct

This project follows the [Contributor Covenant](CODE_OF_CONDUCT.md). By participating,
you agree to uphold it.

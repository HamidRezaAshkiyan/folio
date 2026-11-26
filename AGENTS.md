# AGENTS.md — Folio

## Overview
**Folio** (`v1.10.1`) is a cross-platform **Bullet Journal (BuJo)** companion app backed by plain Obsidian-compatible Markdown files. A single Rust binary provides a 6-pane Ratatui TUI (`folio --tui`), a native Iced desktop GUI (`folio --gui` with Dark/Light themes and live LTR/AUTO/RTL Persian/Arabic shaping), an optional embedded local Web GUI (`folio --web`), and scriptable CLI commands (`log`, `review`, `import`, `export`, `space`, `search`, `index`, `monthly`, `update`). The repository also includes a companion TypeScript Obsidian plugin (`obsidian-plugin/`, id `folio-bujo`) that renders and interacts with the same Folio Markdown convention inside Obsidian.

## Tech Stack & Requirements
- **Core App (Rust):** Edition **2024** (`rustc` 1.85+), single binary `folio`.
  - **TUI:** `ratatui` 0.29 + `crossterm` 0.28.
  - **Native Desktop GUI:** `iced` 0.14 (`advanced-shaping`, `image`).
  - **Local Web GUI:** `tiny_http` 0.12 + `open` 5.3.
  - **CLI & Serialization:** `clap` 4.5 (`derive`), `chrono` 0.4, `serde` / `serde_json`, `toml` 0.8, `dirs` 6.0, `anyhow`.
  - **Dev/Test:** `tempfile` 3.14.
- **Obsidian Plugin (`obsidian-plugin/`):** TypeScript 5.6, `obsidian` API (`minAppVersion: 1.5.0`), `@codemirror/state` & `@codemirror/view` (CodeMirror 6), bundled with `esbuild` 0.24, tested with Node's built-in test runner (`node --test`).

## Repository Structure
```text
folio/
├── Cargo.toml                      # Rust binary manifest (v1.10.1, edition 2024)
├── src/
│   ├── main.rs                     # CLI parser & dispatcher (TUI, GUI, Web, CLI subcommands)
│   ├── core/                       # Shared domain logic & vault persistence
│   │   ├── model.rs                # Signifiers (., ×, >, <, ~, o, =, -), modifiers, rapid-input parser
│   │   ├── markdown.rs             # YAML frontmatter & Folio Markdown serializer/parser
│   │   ├── vault.rs                # Vault layout (Daily Notes, Spaces, Reviews, Future Log, Index)
│   │   ├── review.rs               # Weekly & Monthly review generators
│   │   ├── io.rs                   # Import/Export (md, txt, csv, json, Apple Notes, Notion, Obsidian)
│   │   ├── config.rs               # FolioConfig (~/.config/folio/config.toml & FOLIO_VAULT)
│   │   └── updater.rs              # GitHub Releases self-updater
│   ├── tui/                        # 6-pane Ratatui TUI (app.rs, ui.rs, theme.rs)
│   └── gui/                        # Iced native GUI (native.rs) & local Web GUI (server.rs, index.html)
├── obsidian-plugin/                # Companion Obsidian plugin (`folio-bujo`)
│   ├── package.json / manifest.json / esbuild.config.mjs
│   ├── src/                        # main.ts, model.ts, vault.ts, view.ts, live-preview.ts, reading-view.ts, modals.ts, settings.ts
│   └── tests/model.test.mjs        # Node built-in unit tests
├── packaging/                      # macOS Info.plist/wrapper, Windows Inno Setup (folio.iss), install.sh/ps1
├── assets/                         # App icons (.png, .icns)
└── docs/                           # GitHub Pages site & interactive preview
```

## Architecture & Key Flows
- **Entry Point (`src/main.rs`):** Cleans up leftover binaries from self-updates (`cleanup_old_binary`), parses `Cli` flags/subcommands, resolves the vault path (`--vault` → `FOLIO_VAULT` → `~/.config/folio/config.toml` → `~/Obsidian/BuJo-Vault`), and launches:
  - `folio` / `folio --tui` → `tui::run` (Ratatui 6-pane TUI).
  - `folio --gui` → `gui::native` (Iced desktop app with RTL shaping).
  - `folio --web [--port 3420] [--no-open]` → `gui::server` (`tiny_http` serving `src/gui/index.html`).
  - Subcommands (`log`, `review`, `import`, `export`, `space`, `search`, `index`, `monthly`, `update`) → headless CLI execution.
- **Vault Format (`src/core/vault.rs` & `markdown.rs`):** Plain `.md` files with YAML frontmatter (`date`, `page`, `tags`) and rapid-log lines like `. [08:15] Task text [paper:no]`. Carry-to-paper status is tracked via `[paper:no]` / `[paper:ok]`.
- **Obsidian Plugin (`obsidian-plugin/src/main.ts`):** Mirrors the exact Folio Markdown format (`src/model.ts`) for live preview (CodeMirror 6), reading view post-processing, and a dedicated BuJo sidebar/workspace view.

## Development & Commands
```bash
# Rust app — build, test, run
cargo build --release
cargo test
cargo run -- --tui
cargo run -- --gui
cargo run -- --web --port 3420 --no-open
cargo run -- log ". Test rapid log entry"

# Obsidian plugin (from obsidian-plugin/)
cd obsidian-plugin
npm install
npm run dev          # Watch build via esbuild
npm run build        # Typecheck (tsc -noEmit) + production bundle -> main.js
npm test             # node --test tests/model.test.mjs
```

## Configuration & Environment
- **`FOLIO_VAULT`:** Overrides the default Obsidian vault path (`~/Obsidian/BuJo-Vault`).
- **`~/.config/folio/config.toml`:** User configuration file (vault path, preferences; loaded by `src/core/config.rs`).
- No secrets or API keys required; `folio update` queries public GitHub Releases.

## Conventions & Gotchas
- **Rust Edition 2024:** Requires a recent Rust toolchain (`rustc` 1.85+).
- **Keep Rust & Obsidian parsers in sync:** Any change to Folio signifiers, `[paper:ok|no]` markers, or frontmatter conventions in `src/core/model.rs` / `markdown.rs` must be mirrored in `obsidian-plugin/src/model.ts` (and covered by both `cargo test` and `npm test`).
- **RTL Shaping:** Iced's `advanced-shaping` feature is intentionally enabled for Arabic/Persian RTL rendering; do not drop it when updating `Cargo.toml`.
- **Committed plugin bundle:** `obsidian-plugin/main.js` is tracked for Obsidian plugin distribution; run `npm run build` in `obsidian-plugin/` when modifying TypeScript sources.

# Folio

Minimalistic, cross-platform **Bullet Journal (BuJo)** companion app with a **Ratatui TUI** (`folio --tui`) and an **embedded local Web GUI** (`folio --gui`), backed by plain **Obsidian-compatible Markdown** files.

Designed for rapid logging when you don't have your physical notebook handy — with a built-in **Carry to Paper** queue (`[paper:no]` / `[paper:ok]`) so you can transcribe entries back into your analog spread.

---

## Features

- **Dual Interface (Single Binary)**:
  - `folio --tui` — 6-pane Ratatui terminal UI (`[1] SPACES`, `[2] VAULT`, `[3] RAPID LOG`, `[4] SIGNIFIERS`, `[5] CARRY TO PAPER`, `[6] .MD BUFFER`)
  - `folio --gui` — Zero-dependency embedded local Web GUI (Dark & Light themes)
  - `folio log "..."` — Instant one-liner rapid logging from any shell
- **Full Bullet Journal Method**:
  - `.` **Action** (toggleable to `×` Completed, `>` Migrated, `<` Scheduled, `~` Cancelled)
  - `o` **Event**
  - `=` **Emotion**
  - `-` **Note**
  - **Custom Spaces** (Collections like `Studio & Work`, `Reading & Study`, `Personal`, or any custom space you create)
  - **Carry to Paper** tracking + physical notebook page number (`Notebook p. 42`)
  - **Weekly & Monthly Reviews** (`Reviews/YYYY-Www.md` and `Reviews/YYYY-MM-review.md`)
  - **Future Log** (`Future Log.md`) & auto-maintained **Monthly Index** (`Daily Notes/YYYY-MM-index.md`)
- **Obsidian-Compatible Markdown**:
  - Plain `.md` files with YAML frontmatter and `[[wikilinks]]`
  - Default vault: `~/Obsidian/BuJo-Vault` (configurable via `--vault`, `FOLIO_VAULT`, or `~/.config/folio/config.toml`)
- **Import & Export**:
  - Import from Markdown, Obsidian, Apple Notes, Notion, CSV, JSON, or plain text (`:imp` in TUI, Import modal in GUI, or `folio import`)
  - Export to Markdown, JSON, or CSV (`:exp` or `folio export`)

---

## Installation

### Prebuilt Binaries (macOS, Linux, Windows)

Download the latest release archive for your platform from the [GitHub Releases](https://github.com/HamidRezaAshkiyan/folio/releases) page:
- **macOS (Apple Silicon)**: `folio-macos-arm64.tar.gz`
- **macOS (Intel)**: `folio-macos-x64.tar.gz`
- **Linux (x86_64)**: `folio-linux-x64.tar.gz`
- **Windows (x86_64)**: `folio-windows-x64.zip`

### From Source (macOS, Linux, Windows)

```bash
git clone https://github.com/HamidRezaAshkiyan/folio.git
cd folio
cargo install --path .
```

Or build a release binary:

```bash
cargo build --release
./target/release/folio --tui
```

---

## Quick Start

```bash
# Launch the Terminal UI (default)
folio
folio --tui

# Launch the embedded Web GUI (opens in browser)
folio --gui
folio --gui --port 3420 --no-open

# Use a custom Obsidian vault path
folio --vault ~/Obsidian/My-Vault --tui

# One-line rapid logging from the shell
folio log ". Finalize studio lighting sketch"
folio log "o Coffee with Maya at Blue Bottle"
folio log "= Calm and unhurried after walking without headphones"
folio log "- Living spaces come from small repairs made daily"

# Log into a custom Space
folio log -s "Reading & Study" "- Finish Pattern Language chapter 4"

# Generate Weekly or Monthly Review
folio review weekly
folio review monthly

# Import / Export notes
folio import ~/Desktop/apple-notes.txt
folio export --format json --output today.json
```

---

## TUI Keybindings

| Key | Action |
| --- | --- |
| `Ctrl+N` / `i` | Focus rapid-log prompt (`:log [.] >`; prefix `. `, `o `, `= `, `- `, `*. `, `!- `) |
| `/` / `Ctrl+F` | Instant Vault Search across all Daily Notes, Spaces, Reviews, and `#tags` |
| `[` / `]` / `t` | Previous Day / Next Day / Today |
| `Tab` | Cycle through Spaces |
| `j` / `k` / Mouse | Navigate, click panes/entries/files, or scroll with mouse wheel |
| `Space` | Toggle Action `.` ↔ Completed `×` |
| `*` / `!` | Toggle Priority (`*`) / Inspiration (`!`) modifier |
| `m` / `M` | Mark selected / all entries copied to physical paper (`[paper:ok]`) |
| `>` / `<` | Smart Migration Picker (Tomorrow / Space / Future Log) / Quick Future Log |
| `1`–`4` / `#` / `0` | Filter by `.` / `o` / `=` / `-` / cycle `#tag` filter / `ALL` |
| `e` / `d` | Edit / Delete selected entry |
| `w` / `r` | Generate & open Weekly / Monthly Review |
| `:w` / `:q` / `:wq` | Save `.md` / Quit |
| `:space <name>` | Create & open a custom Space |
| `:page <num>` | Set physical Notebook page number |
| `:imp <path>` | Import notes from file |
| `:exp <fmt> <path>` | Export current document (`md`, `json`, `csv`) |
| `?` | Toggle Help overlay |

---

## Obsidian Vault Structure

```text
~/Obsidian/BuJo-Vault/
├── Daily Notes/
│   ├── 2026-11-21.md
│   └── 2026-11-index.md
├── Spaces/
│   ├── studio-work.md
│   ├── reading-study.md
│   └── personal.md
├── Reviews/
│   ├── 2026-W47.md
│   └── 2026-11-review.md
└── Future Log.md
```

Example `Daily Notes/2026-11-21.md`:

```markdown
---
date: 2026-11-21
page: 42
tags:
  - bujo
  - daily
---
. [08:15] Finalize studio lighting sketch before Monday critique [paper:no]
o [09:30] Coffee with Maya at Blue Bottle — discussed zine layout [paper:ok]
= [10:45] Calm and unhurried after walking without headphones [paper:no]
- [11:10] Christopher Alexander: living spaces come from small repairs made daily [paper:no]
× [11:25] Order fountain pen ink refill (Iroshizuku Shin-kai) [paper:ok]
```

## License

MIT

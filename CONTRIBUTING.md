# Contributing to GEDRoots

GEDRoots is written in Rust with [GPUI](https://www.gpui.rs/) (through
[GPUI Kit](https://gpui-kit.com)). Before you start, read [`AGENTS.md`](AGENTS.md) for the
project rules (GEDCOM 7 first, generic features, translated interface, cross-platform) and
[`design.md`](design.md) before any interface work.

## Development environment

The tools (pinned Rust, with rustfmt and clippy) and the project's commands are described in
[`mise.toml`](mise.toml), the exact versions in `mise.lock`. With [mise](https://mise.jdx.dev):

```sh
mise trust && mise install   # once: installs the project's toolchain
mise tasks                   # list of tasks
mise run app tree.ged        # launch the app (optimized build)
mise run ci                  # formatting, tests and clippy, like the CI
mise run fmt                 # format
```

On Linux, first install GPUI's dependencies:
`libxkbcommon-dev libxkbcommon-x11-dev libfontconfig-dev libwayland-dev libvulkan-dev`.

Machine-specific settings go in `mise.local.toml` (not versioned), for example a build folder
outside the repository:

```toml
[env]
CARGO_TARGET_DIR = "{{env.HOME}}/Developer/gedroots-target"
```

To change the Rust version: edit `mise.toml`, then
`mise lock --platform linux-x64,macos-arm64`.

## Run from source

```sh
mise run app path/to/tree.ged
```

Without an argument, the app offers to open a file or create a new one. Optional environment
variables (map tiles, geocoder, language, appearance) are listed in the
[documentation](website/docs/reference/environment.mdx).

## Tests

```sh
mise run test
# Consistency check and statistics of a file, on the command line:
mise exec -- cargo run -p gedcom --example analyse -- ~/Genealogy/tree.ged
# Data Quality issues of a file:
mise exec -- cargo run -p gedcom --example quality -- ~/Genealogy/tree.ged
# Check that a real file is read and written back identically:
GEDROOTS_TEST_GED=~/Genealogy/tree.ged mise run test real_file -- --nocapture
```

Always test on a copy of a real file, never on the original. The sample file
`crates/gedcom/tests/fixtures/example.ged` is fictional; personal `.ged` files are ignored by
git. Never commit a `.ged` file that contains living people.

## CI

The CI ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)) runs the same tasks:
`mise run ci` on Linux, and `mise run bundle --dmg` on macOS (on `main` or on demand), whose
`.dmg` can be downloaded from the artifacts. mise-action's cache keeps the toolchain (rustup
lives in mise's folder), and a cache keyed on `Cargo.lock` keeps the crates and `target/`.

## Packaging

- **macOS**: `mise run install` builds `GEDRoots.app` and copies it to `/Applications`.
  `mise run bundle` only makes `target/release/GEDRoots.app`; `mise run bundle --dmg` also
  makes a disk image (used by the CI). The app is signed ad hoc, which is enough on the Mac
  that built it. Distributing it without a Gatekeeper warning requires a Developer ID
  signature and Apple notarization.
- **Linux**: `mise run install` installs `~/.local/bin/gedroots`, the icon and the
  applications menu entry.
- **Windows**: `cargo build --release -p gedroots` produces `target\release\gedroots.exe`,
  which opens without a console window.

These tasks are scripts in [`mise-tasks/`](mise-tasks); `mise run bundle --help` lists their
options. The icon (`packaging/icon.png`) comes from the GEDRoots brand kit.

## Code layout

| Crate | Role |
| --- | --- |
| [`crates/gedcom`](crates/gedcom) | Lossless GEDCOM 7 reading and writing, typed views (people, families, sources, places, media), `CROP` (`crop.rs`), multi-page media (`multipage.rs`), web links (`weblink.rs`), import assistant (`import.rs`), GEDZIP export (`gedzip.rs`), GEDCOM 5.5.1 and Geneanet export (`gedcom551.rs`), search sites (`websearch.rs`), dates (`date/`), coordinates (`geo.rs`), analysis (`analysis/`), Data Quality checks (`quality.rs`), search (`search.rs`, `query.rs`, `phonetic.rs`, `saved.rs`), relatives and media links (`relations.rs`), source records (`act.rs`), agent proposals (`proposal.rs`, `research.rs`) and its own translations (`locales/`). |
| [`crates/gedroots`](crates/gedroots) | The GPUI application: `commands.rs` (every command, shortcut and menu), `workspace.rs` and `workspace/` (window, views, charts, dialogs, Data Quality and Statistics pages, review of agent proposals), `editor.rs` (visual and raw editors), `act_editor.rs` (record editor), `agent/` (MCP server and prompts), `map.rs` (place map), `theme.rs` (design tokens), `locales/` (English and French texts). |

Reference documents for contributors: [GEDCOM extensions](docs/extensions.md),
[localization](docs/localization.md), [AI agents](docs/agents.md).

## User documentation

The user documentation lives in [`website/`](website) and is built with
[Blume](https://useblume.dev). See [`website/README.md`](website/README.md) for how to preview
it, the writing guidelines and how screenshots are made. When you change a feature, update the
page that describes it in the same pull request.

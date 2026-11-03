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

The Test workflow ([`.github/workflows/test.yml`](.github/workflows/test.yml)) runs the same
task as a local machine, `mise run ci`, on Linux. The installers are built by the Release
workflow (see Releases), also on pull requests that touch packaging, whose packages can be
downloaded from the artifacts. mise-action's cache keeps the toolchain (rustup
lives in mise's folder), and a cache keyed on `Cargo.lock` keeps the crates and `target/`.

## Packaging

- **macOS**: `mise run install` builds `GEDRoots.app` and copies it to `/Applications`.
  `mise run bundle` only makes `target/release/GEDRoots.app`; `mise run bundle --dmg` also
  makes a disk image. The app is signed ad hoc, which is enough on the Mac
  that built it. Distributing it without a Gatekeeper warning requires a Developer ID
  signature and Apple notarization.
- **Linux**: `mise run install` installs `~/.local/bin/gedroots`, the icon and the
  applications menu entry.
- **Windows**: `cargo build --release -p gedroots` produces `target\release\gedroots.exe`,
  which opens without a console window.

These tasks are scripts in [`mise-tasks/`](mise-tasks); `mise run bundle --help` lists their
options. The icon (`packaging/icon.png`, and `packaging/windows/gedroots.ico` built from it)
comes from the GEDRoots brand kit.

## Releases

Releases follow [mise](https://github.com/jdx/mise)'s format and are fully automated by
[`.github/workflows/release.yml`](.github/workflows/release.yml):

1. After each merge into `main`, the **`chore: release X`** pull request is opened or updated
   (`mise run release:pr`). It sets the next version, in calendar versioning like mise
   (`2026.11.0`, then `2026.11.1`; the month restarts at `.0`), and adds its entry to
   [`CHANGELOG.md`](CHANGELOG.md) with [git-cliff](https://git-cliff.org) (`cliff.toml`).
   Pull request titles in [Conventional Commits](https://www.conventionalcommits.org) form
   (`feat(places): …`, `fix: …`) are grouped by kind; other titles go under “Changed”.
2. Merging it tags `vX` and creates a draft GitHub release (`mise run release:tag`).
3. On macOS, Windows and Linux, `mise run package --upload` builds the installers and the update
   packages with [Velopack](https://velopack.io) and adds them to the draft: `GEDRoots.dmg` and
   `GEDRoots-osx-Setup.pkg`, `GEDRoots-win-Setup.exe`, `GEDRoots.AppImage`, the `.nupkg`
   packages and their `releases.<channel>.json` feed.
4. [communique](https://github.com/jdx/communique) writes the release notes for users
   (`communique.toml`, `ANTHROPIC_API_KEY` secret; git-cliff's entry otherwise) and the release
   is published (`mise run release:publish`).

The app reads the feed of the latest release to update itself
([`crates/gedroots/src/updates.rs`](crates/gedroots/src/updates.rs)). The download links of
the README and the documentation point to `releases/latest/download/<file>`, so the asset names
never contain the version.

The release tools are in [`mise.release.toml`](mise.release.toml), loaded with
`MISE_ENV=release`: `MISE_ENV=release mise install`, then `MISE_ENV=release mise run package`
builds the packages for your system in `target/dist` without uploading them.

The packages are not signed with a trusted certificate: macOS asks to confirm the first launch
in System Settings › Privacy & Security, and Windows SmartScreen shows a warning. Signing them
needs an Apple Developer ID with notarization (`vpk pack --signAppIdentity … --notaryProfile …`)
and a Windows code signing certificate (`--azureTrustedSignFile` for Azure Artifact Signing).

On macOS they are signed with GEDRoots' self-signed certificate, stored in the
`MACOS_CERTIFICATE` and `MACOS_CERTIFICATE_PASSWORD` secrets. macOS remembers the permissions
given to an app (folders, local network…) for its code identity, and an ad hoc signature has
none that lasts: each version would be a new app, asked for its permissions again after every
update. `mise run release:certificate` creates the certificate and the secrets, once: a new
certificate is a new identity, so it must never be replaced while the old one works. Without the
secrets, `mise run package` signs ad hoc; pull requests from forks use a throwaway certificate
to test the signature.

## Code layout

| Crate | Role |
| --- | --- |
| [`crates/gedcom`](crates/gedcom) | Lossless GEDCOM 7 reading and writing, typed views (people, families, sources, places, media), `CROP` (`crop.rs`), multi-page media (`multipage.rs`), web links (`weblink.rs`), import assistant (`import.rs`), GEDZIP export (`gedzip.rs`), GEDCOM 5.5.1 and Geneanet export (`gedcom551.rs`), search sites (`websearch.rs`), dates (`date/`), coordinates (`geo.rs`), analysis (`analysis/`), Data Quality checks (`quality.rs`), search (`search.rs`, `query.rs`, `phonetic.rs`, `saved.rs`), relatives and media links (`relations.rs`), source records (`act.rs`), agent proposals (`proposal.rs`, `research.rs`) and its own translations (`locales/`). |
| [`crates/gedroots`](crates/gedroots) | The GPUI application: `commands.rs` (every command, shortcut and menu), `workspace.rs` and `workspace/` (window, views, charts, dialogs, Data Quality and Statistics pages, review of agent proposals), `editor.rs` (visual and raw editors), `act_editor.rs` (record editor), `agent/` (MCP server and prompts), `map.rs` (place map), `save_dialog.rs` (native save dialog with a format list), `theme.rs` (design tokens), `locales/` (English and French texts). |

Reference documents for contributors: [GEDCOM extensions](docs/extensions.md),
[localization](docs/localization.md), [AI agents](docs/agents.md).

## User documentation

The user documentation lives in [`website/`](website) and is built with
[Blume](https://useblume.dev). See [`website/README.md`](website/README.md) for how to preview
it, the writing guidelines and how screenshots are made. When you change a feature, update the
page that describes it in the same pull request.

# Instructions for agents

GEDRoots is free, local genealogy software (Rust, GPUI through gpui-kit) whose data stays in
a **GEDCOM 7** file. These rules apply to every agent working in this repository (Claude Code,
Codex, Copilot…).

**Language.** Everything technical is in English: code, comments, documentation, GEDCOM
extension tags, search keywords, commit messages and pull requests. The interface is written in
English and translated into French. Antonin writes in French: replies to him are in French.

## Design: read before any interface work

[`design.md`](design.md) is the single reference: research process, principles, tokens
(`crates/gedroots/src/theme.rs`), interaction rules and the differences between macOS, Windows
and Linux. Read it **in full** before proposing a mockup or touching a view.

Before any design proposal or mockup, without waiting to be asked:

1. look at how the best genealogy software handles the subject;
2. look at how the best apps in general handle the same pattern;
3. check the result against the Apple, GNOME and Windows HIGs and against `design.md`;
4. present this research (short, with links or screenshots) with the mockup.

The details (which apps, what to hand in) are in [design.md › Before any mockup](design.md#before-any-mockup).

## Project rules

- **Standard GEDCOM 7 first.** What the app must remember (root person, file settings…) goes
  in the `.ged` as `_GR_*` tags declared in `HEAD.SCHMA` and documented in
  [`docs/extensions.md`](docs/extensions.md), never in a sidecar file. No GEDCOM X.
- **Generic features.** Nothing is tailored to a particular file: data fixes go through the
  import wizard (`crates/gedcom/src/import.rs`, pluggable checks).
- **Cross-platform** (macOS, Windows, Linux): prefer solutions that work everywhere.
- **Commands**: every command lives in the menu model (`crates/gedroots/src/commands.rs`) and
  therefore in the `⌘⇧P` palette; no toolbar (see `design.md`).
- **Translated interface text**: the app is written in English and translated into French. No
  displayed text is hardcoded: a key in `crates/*/locales/*.yml` with `en` and `fr`, read with
  `t!("…")` (see [`docs/localization.md`](docs/localization.md)). Search keywords
  (`born:`, `without:source`…) are a technical language and stay in English; only their labels
  are translated.
- **GEDCOM identifiers hidden** in the interface, except Edit › Copy ID and the advanced
  search.
- **Test on a copy** of a real file, never on the original. Never commit a `.ged` containing
  living people.
- **One PR at a time**, clean, merged only when Antonin approves it.
- Tools and CI: `mise` (`mise.toml`). Before pushing: `mise run ci` (formatting, tests,
  clippy); `mise run app file.ged` launches the app.

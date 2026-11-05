# Localization

GEDRoots is written in English and translated into French. Every string shown in the
interface comes from a catalog: `crates/gedroots/locales/` for the app's views and commands,
`crates/gedcom/locales/` for the texts the data library produces (event names, dates, kinship,
data quality, search labels…). The code never contains user-facing text.

## Library

[rust-i18n](https://github.com/longbridge/rust-i18n), the library gpui-component already uses
for its own widgets:

- translations are compiled into the binary (no files to ship, nothing to load at startup);
- one global language for the app and its widgets, so gpui-component's calendar or text field
  menu follow the same setting (we add French to them in `locales/gpui_component.yml`);
- YAML catalogs with every language side by side, easy to review in a pull request.

[Fluent](https://projectfluent.org/) was the other candidate. Its plural and gender rules are
richer, but it would be a second translation system next to gpui-component's, and the few
strings that depend on a number are handled with explicit keys (see below).

## Choosing the language

At startup (`crates/gedroots/src/i18n.rs`), GEDRoots takes the first language it supports in
the system's list of preferred languages, and falls back to English:

- macOS: System Settings › General › Language & Region, including the per-app language
  (`CFBundleLocalizations` in `packaging/macos/Info.plist` lists ours);
- Windows: the display language;
- Linux: `LANGUAGE`, `LC_ALL`, `LC_MESSAGES`, `LANG`.

`GEDROOTS_LANG=en mise run app` forces a language, handy to check a screen in both.

## Adding a string

1. Pick the catalog of the area (`commands.yml` for menus and commands, one file per view or
   module) and add a key with **both** languages:

   ```yaml
   cmd:
     add_event:
       en: Add Event…
       fr: Ajouter un événement…
   ```

2. Use it with the crate's `t!` (`rust_i18n::t!` plus French elision, see 3), which returns a
   `Cow<str>` that converts to `SharedString`:

   ```rust
   use crate::t;
   menu.item(act(&ws, t!("cmd.add_event"), …))
   ```

3. Values go in named placeholders, never by concatenation, so each language can order the
   sentence its own way: `t!("person.age", years = n)` with `en: "%{years} years old"`.
   In French, a “de” written just before a placeholder is elided when the value starts with a
   vowel: `"Décès de %{name}"` gives « Décès d’Antonin GUYOT » (`gedcom::i18n::elide`). Write
   “de”, never “d’”, in front of a placeholder.

`mise run test` checks that every key exists in every language with the same placeholders,
that every key used in the code exists, and that no key is left unused.

Unit tests run in English, since the language is global to the process; a French text is
tested with an explicit language (`t!("key", locale = "fr")` or the `_in("fr", …)` functions).

## What is not translated

- Search keywords (`born:1850`, `without:source`, `sex:f`) and template variables
  (`{surname}`, `{given}`) are a technical language: always in English, whatever the
  interface language. Only the labels shown for them are translated.
- Dates are accepted in French and English whatever the interface language; the text the app
  writes into the file (record titles, media file names, role phrases) follows the interface
  language.

## Style

- English follows the platforms' conventions: title case for menu items, buttons and window
  titles (“Add Event…”), sentence case for everything else.
- The French text is the one the app had before the switch to English: keep it as is unless
  the wording itself changes.
- Genealogy vocabulary: person (not individual) in the interface, record for a civil or church
  record (*acte*), repository (*dépôt*), root person (*individu racine*).
- Numbers that change a word get one key per form (`one` and `other`); remember that French
  uses the singular for 0 and 1.

## Adding a language

Add its code to `LANGUAGES` in `crates/gedroots/src/i18n.rs` and to `CFBundleLocalizations`,
then a line for it under every key: the tests list the missing ones.

# Atlas

The [Atlas](https://github.com/gedroots/atlas) says, country by country, what genealogy records
were kept, from when to when, who may read them, and the legal age at marriage. It lives in its
own repository, under CC0, with its JSON Schema and the documentation of the format, so that other
software can reuse it.

GEDRoots includes it as a git submodule, `crates/gedcom/atlas`, pinned to one commit, and
`crates/gedcom/build.rs` embeds its files in the app, which works offline. Clone with
`git clone --recursive`, or run `git submodule update --init` in an existing clone.

GEDRoots uses it to estimate when a marriage took place (the legal age of the country) and to say
where to look for a record ([agents](agents.md#research-leads-and-log); `crates/gedcom/src/leads.rs`).
The country of a place is the last part of the place that names a country of the Atlas; a place
that names none takes the file's most frequent country.

## Updating the data

1. Change the data in gedroots/atlas, through a pull request there.
2. Here, move the submodule to the new commit (`git -C crates/gedcom/atlas pull origin main`) and
   open a pull request: `mise run ci` checks that every file reads and that the leads still pass
   their tests.

A new field or record type in the Atlas also needs its support in `crates/gedcom/src/atlas.rs`
(unknown fields are refused).

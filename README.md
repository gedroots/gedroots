# GEDRoots

**Free and local** genealogy software, written in Rust with [GPUI](https://www.gpui.rs/)
(via [GPUI Kit](https://gpui-kit.com)). Your data stays in a **GEDCOM 7** file on your disk:
no database, no account, no cloud.

The interface covers the main features of [Ancestris](https://www.ancestris.org) and follows
the [GEDRoots design system](design.md) (neutral backgrounds with touches of violet, Inter
typeface, light or dark following the system). It is written in English and translated into
French (see [docs/localization.md](docs/localization.md)).

![Pedigree chart](docs/capture-arbre.png)

## Features

- **Navigation**: a sidebar with four zones (People, Sources, Places, Media) and their counts;
  hide it with `⌃⌘S` (`Ctrl+Alt+S`). Repositories and notes open from the records that cite
  them, from search or from the View menu; so do Quality and Statistics.
- **Everything is in the menu bar** (File, Edit, Person, Source, Media, View, Help), once per
  command: no toolbar, the screen is for the data. Native menus on macOS; on Linux and Windows,
  the same menus at the top of the window. Commands that do not apply to the selection are
  greyed out (macOS) or do nothing.
- **Command palette** (`⌘⇧P` / `Ctrl+Shift+P`): every applicable command, accent-insensitive
  search. Shortcuts: `⌘1`…`⌘4` sidebar zones, `⌘5` repositories, `⌘6` notes, `⌘7` quality,
  `⌘8` statistics, `⌥⌘1`…`⌥⌘3` chart/record/GEDCOM (the last one opens in a tab you can
  close), `⌘F` search, `⌘⇧N` new person, `⌘⇧H` root person, `⌘=` / `⌘-` fan chart
  generations, `⌘⇧S` save as.
- **Right-click everywhere**: list rows (and the background, to create), chart and fan chart
  boxes, empty boxes (new person or existing person), events, relatives, links and thumbnails
  of the detail panel, map markers and background (“Place This Place Here”). The menu acts on
  the clicked item without selecting it, with the same commands and labels as the menu bar;
  only actions on part of a record (an event, an attached media, a member) exist only on
  right-click.
- **Editable events**: clicking an event (person or union) reopens it in the same window used
  to add it (type, detail, date, suggested place, source); it can also be deleted. Notes, age,
  media and other citations of the event are kept.
- **Unified timeline**: a person's record shows their events and those of their unions
  (marriage, divorce… “with” the spouse) in a single list by date, with their age at each
  event; a marriage can also be added there directly. The button next to “+” (or View ›
  Relatives' Events) mixes in the births and deaths of their parents, siblings, spouses,
  children and grandchildren during their lifetime. Each event shows its media (`2 OBJE`,
  standard GEDCOM 7): dropping files on the event attaches them, right-click attaches, links an
  existing media or detaches.
- **Search**: one field per list, ignoring accents and case. Each complete criterion becomes a
  token (“Born 1800–1850”, “Without Sources”) and, while typing, suggestions taken from the file
  (places, names, occupations, record types, repositories…) show their number of results (↑ ↓
  ↵, Esc). The zone's counter in the sidebar gives the number of results. Type ordinary words:
  “1850” suggests “Born in 1850”, “Born about 1850” (± 5 years), “Died about 1850”, “Alive in
  1850”; a given name and a surname (“lucien guyot”) suggest their ancestors, descendants,
  spouses, children, parents and siblings (also in Person › Search Ancestors / Descendants).
  Clicking a token sets the year margin (exactly, ± 2, ± 5, ± 10 years) or the spelling variants
  of a name (Guyot, Guyau, Guillot; French phonetic key, `phonetic.rs`), and removes it.
  Criteria keywords are in English whatever the interface language (only their labels are
  translated). Criteria for people: `born:1800-1850`, `born:1850±5`, `died:<1900`,
  `alive:1870`, `place:troyes`, `name:guyot` (variants included; `name:=guyot`: exact),
  `occupation:miller`, `sex:f`, `ancestors:@I1@`, `descendants:@I1@`, `spouses:@I1@`,
  `children:@I1@`, `parents:@I1@`, `siblings:@I1@`, `without:source`, `without:parents`,
  `without:marriage`, `with:media`, `sosa:1-63`…; for sources: `type:birth`,
  `repository:…`, `without:media`, `without:citation`; for places: `in:aube`,
  `without:coordinates`; for media: `event:birth`, `person:guyot`, `format:pdf`,
  `medium:photo` (`MEDI`), `without:link`. A value with spaces is written in quotes. In the
  Media zone, a row of types (All · Records · Portraits…) follows the subfolders of the
  settings, otherwise the folders of the files, otherwise the GEDCOM medium (`MEDI`); web links
  have their own type.
  **Saved searches**: the “+” of the sidebar's Searches section (or View › Save Search) keeps
  the current search under the name of its tokens, ready to rename; it shows its number of
  results and lives in the `.ged` (`HEAD._GR_FIND`, see
  [extensions](docs/extensions.md#_gr_find)).
- **People**: quick filters (View › Filter List); record with portrait, image gallery, events
  (dates in the interface language, French Republican calendar included), cited sources,
  clickable family (parents, spouses, children).
- **No Families tab**: a union (`FAM`) is managed from the record of either spouse (union
  events, children, photos and notes of the couple). With several unions, the “+” window offers
  “Child with …” for each. A spouse added to a single parent joins their existing union. A link
  to a family (Quality, “Cited by”…) opens the record of a spouse.
- **Charts**: **free chart** (ancestors on the right, descendants and spouses on the left,
  siblings above; move around by dragging or with the wheel, generations load as you go in
  both directions, a minimap shows where you are, double-click to recenter) and **fan chart**
  (2 to 7 generations).
- **Add a relative** in one click from a person (father, mother, spouse, child, sibling), or
  **link an existing person** through a picker that suggests plausible dates first; detach a
  person from a family. `FAMS`/`FAMC` ↔ `HUSB`/`WIFE`/`CHIL` links stay consistent, and an
  emptied family is deleted.
- **Photos and records**: drag and drop or choose files on a person or a source; suggestions
  of files from the folder (`Actes/`, `Portraits/`…) whose name contains the person's; files in
  the folder without media are in Quality › Files to Attach (Media › Files to Attach menu).
- **Sources, repositories, media, shared notes**: list, visual editor, and a “Used by / Links”
  panel to move between records.
- **Places**: index of every `PLAC` in the file, global renaming and coordinates (`MAP`). While
  entering a place, the menu suggests the file's places, a locality within a known place (“La
  Borde, Troyes”), real places from OpenStreetMap (Photon, while typing) and “Create “…””. A new
  place opens in the “New Place” window: one field per jurisdiction of the file's place format
  (File Settings › Places), already filled in, and a map where a click drops the pin.
  Coordinates go in `PLAC.MAP`, the OpenStreetMap identifier in `PLAC.EXID` (standard GEDCOM 7).
- **Place map**: every place with coordinates on an OpenStreetMap map (drag to move, wheel to
  zoom, click a marker to open the place). Places without coordinates are located one at a
  time from their record (“Locate” suggests matching places, you pick the right one) or from
  Quality › Places Without Coordinates, or placed by hand (“Place on Map”, or right-click on
  the map); coordinates are written in the GEDCOM (`PLAC.MAP.LATI/LONG`, format `N49.11652` /
  `W1.11437`) and can be undone like any change. Tiles are cached on disk: in “Offline” mode,
  nothing goes out to the Internet.

![Place map](docs/capture-carte.png)

- **Media / photos**: portraits of people, images of sources (scanned records) in the record
  and the detail panel, paths relative to the `.ged` file.
- **Cropping and identifying people**: on a photo's record, Media › “Identify a Person in the
  Photo”, then draw a frame around the face and choose the person. This is the standard GEDCOM 7
  `CROP` on the `INDI → OBJE` link (no extension, readable by other software): the group photo
  shows the names, and the person's portrait and thumbnails are cropped to their face. Each link
  to an image (source, family…) can also be bounded, for example the useful part of a register.
- **Web links** (a record's page in online archives, a Geneanet profile…): “Web Links” section
  of the detail panel, clickable (opens in the browser), with “+”, edit and remove; also Edit ›
  “Add Web Link…”. Stored in standard GEDCOM 7: a multimedia link to an `OBJE` whose `FILE` is
  the URL (`FORM text/html`), and `WWW` for repositories.
- **Import assistant** (File › “Import GEDCOM…”): opens a file from other software, lists what
  it contains that is not standard and offers to convert it to GEDCOM 7 (undoable). First
  check: `_URL` tags become multimedia links (`WWW` for repositories); events with an
  extension tag (`_MILT`, `_MILI`, `_DEG`…) become `EVEN` + `TYPE`. The checks are in
  `crates/gedcom/src/import.rs`.
- **Export** (File › Export) in three formats.
- **GEDZIP with media**: a `.gdz` archive (GEDCOM 7, chapter 4) with `gedcom.ged` and the media
  files, to pass on the complete tree. Relative paths keep their folder (`Actes/…`), files
  outside the .ged's folder go in `media/`, `FILE`s are rewritten for the archive; the .ged and
  the media are not modified. Missing files keep their `FILE` and are listed at the end, each
  leading to its media.
- **GEDCOM 5.5.1**: a `.ged` for software that does not read GEDCOM 7 yet (5.5.1 header,
  `SNOTE` → `NOTE`, `@#DFRENCH R@` calendars, `ROLE` → `RELA`, MIME types → formats, structures
  specific to GEDCOM 7 and `_GR_*` extensions removed, doubled `@`, long lines as `CONC`).
  Conversion in `crates/gedcom/src/gedcom551.rs`.
- **GEDCOM for Geneanet**: the same export, adapted to Geneanet's (GeneWeb) import: when the
  first jurisdiction of the place format is a locality (`Lieudit`, `Hameau`…), places become
  `[Locality] - Town, …`; witness roles in GeneWeb's vocabulary (`GODP`, `Civil officer`…).
  Geneanet's import page then opens in the browser. Geneanet has no public API: uploading stays
  manual.
- **Search on** (Person, and right-click on a person): opens the person's search on a website
  in the browser (Geneanet and FamilySearch by default). Sites are set in File Settings ›
  Search: a name and a URL template whose variables (`{surname}`, `{given}`, `{birth}`,
  `{death}`, `{place}`, parents, spouse) are replaced with the person's data. Stored in the .ged
  (`_GR_WEB`).
- **File settings** (⌘ / Ctrl + `,`; GEDRoots menu on macOS, File elsewhere): author, e-mail,
  copyright, description, language and root person. Everything is stored in the .ged's header
  in standard GEDCOM 7 (`HEAD.SUBM`, `COPR`, `NOTE`, `LANG`). On each save, `HEAD.SOUR` and
  `HEAD.DATE` record GEDRoots and the date.
  - **Places**: place format (`HEAD.PLAC.FORM`) — jurisdictions to rename, reorder, add or
    remove, with every place in the file rewritten (undoable) and a preview. A place's record
    has one field per jurisdiction, and short labels (“Troyes (Aube)”) follow this format.
  - **Events**: custom event types (`EVEN` + `TYPE`), found in the file, to rename or merge,
    with a label shown apart from the `TYPE` (“Military Service” for `TYPE military service`);
    a new type or a label is declared by the [`_GR_EVEN`](docs/extensions.md#_gr_even)
    extension. They are offered in “Add Event…”.
  - **Media**: media folder and subfolders (Records, Portraits…), each with the events it
    receives (checkbox menu) and its name template (`{sosa:6}_{branch}_{type:1}_{surname}_{given}`…,
    Sosa-d'Aboville numbers computed from the root), extension
    [`_GR_MEDIA`](docs/extensions.md#_gr_media), with an example per subfolder. A file attached
    from outside is copied into place; Media › “Rename by Template…” files the files of an
    existing media.
- **Multi-page media**: a record on two sheets is a single media (`OBJE` with one `FILE` per
  page, standard GEDCOM 7). Page-by-page viewer (thumbnails, ⇞ / ⇟), pages added by drag and
  drop or Media › Pages, reordered by dragging thumbnails, removed or split into a new media. To
  merge existing media: ⌘ / Ctrl + click or Shift + click in the media list, then “Merge” (or
  right-click, or Media › Merge with Another Media…). Every link is rewritten; a person framed
  on page 2 keeps their frame (extension [`_GR_FILE`](docs/extensions.md#_gr_file)).

![People identified in a photo](docs/capture-photo.png)
- **Root person**: stored in the file itself (extension `_GR_ROOT`, see
  [docs/extensions.md](docs/extensions.md)), never in a sidecar file.
- GEDCOM identifiers do not appear in the interface: links are chosen by name (spouse,
  repository…), the identifier is available through **… → Copy ID** and the GEDCOM tab.
- **Visual editor** for common records (person, source, repository, media, note): only the
  fields shown are modified, everything else is kept.
- **Raw GEDCOM editor** for every record, with validation; changing an identifier updates every
  pointer. **Autocompletion** from the official GEDCOM 7 schema: tags allowed at this place
  (also searchable by label in the interface language, “birt” or “nais” → `BIRT`),
  people/families/sources for pointers, enumerated values, date keywords and months (French
  Republican calendar included), places already entered. `Ctrl+Space` shows suggestions on
  demand.
- **Records in the source's page**: a source's page lists its records (read back from the
  citations that point to it, grouped by `PAGE` reference); “+” or `⌘E` / `Ctrl+E` opens an
  empty record, a click reopens a record to correct it, right-click deletes it. “New Source”
  opens directly on an empty record. The editor is generic, without templates: choose any
  GEDCOM event (or a custom type), then add the people with their standard role (child,
  father, mother, husband, witness, godfather, informant, officiant, neighbor…); fields and
  family links follow from the role. For each person, GEDRoots suggests the existing people who
  match (name, age, and links already known: “parent of…”, “spouse of…”); you can link, create,
  or mention without creating. Saving writes the event, family links, witnesses (`ASSO`), dated
  occupations and residences, a birth estimated from the age, and every citation with its role
  (`EVEN`/`ROLE`) right away, without any extension. GEDCOM preview before writing, and “Save
  and Next Record” to go through a register (the reference number advances by itself).
  Images or PDF of the record: a media of the file (“Existing Media…”, search by title), or
  files dropped anywhere on the editor (or “Other File…”); a file already imported reuses its
  media, without duplicates; several files make a single multi-page media. On saving, they are
  named and filed according to File Settings › Media: copied if they come from elsewhere,
  moved if they are already in the folder and no other media uses them.
- Create / delete a record (with confirmation; a place can be removed from every event), undo
  (50 levels), `CHAN` change date updated as in Ancestris.
- **Data quality** (`⌘7` / `Ctrl+7`, View menu): a page with the anomalies to fix, by category:
  impossible dates and ages, inconsistent family links, GEDCOM 7 structure (tag not allowed,
  required information missing or duplicated, unreadable date, unknown value, broken link),
  empty or isolated items (empty events and notes, families without members, records cited
  nowhere, trees separated from the root), media (file not found, moved within the folder,
  duplicate media, missing format), events without sources, files of the folder to attach,
  places (variants of the same place, inconsistent hierarchy such as “Loire” sometimes in
  “Rhône-Alpes”, sometimes in “Auvergne-Rhône-Alpes”, unreadable coordinates) and places without
  coordinates. A click opens the item (`⌘[` goes back), the row's button fixes it when that is
  safe (delete, restore a one-way link, remove a broken link, fix the path, merge, create the
  media, harmonize), also in bulk, with confirmation for deletions and merges. “Locate…”
  chooses the file of a missing media by hand, or opens the place on the map to locate it.
  Right-click › “Hide” writes [`_GR_IGNORE`](docs/extensions.md).
- **Relationship** (Person › Relationship With… menu): “second cousin”, “first cousin once
  removed”, half-brother, father-in-law… (in French: « cousine issue de germain », « oncle à la
  mode de Bretagne »), with the path through the common ancestor and the civil degree, shown in
  the person's panel.
- **Statistics** (`⌘8` / `Ctrl+8`, View menu): a page with frequent surnames and given names,
  longevity by century, age at marriage, completeness of the root person's ancestry by
  generation, and pedigree collapse.
- **AI agents** (File › Connect an AI Agent…): a local MCP server, on `127.0.0.1`, lets an
  agent such as Claude Code read the open file (records, search, records to look for among the
  ancestors) without ever writing it: the records it finds wait in the “To Review” page, where
  you compare what will be written with the tree before saving. Living people are hidden. Two
  prompts, `find_record` and `read_record`, give the agent the method to look for a record and
  then propose it. See [docs/agents.md](docs/agents.md).
- **Lossless** saving: a file read and written back is identical to the byte (proprietary tags
  `_SOSADABOVILLE`, `_TODO`… included). A `.ged.bak` copy is kept on each save.

![Fan chart](docs/capture-eventail.png)

![Link an existing person](docs/capture-lier.png)

![Visual editor](docs/capture-fiche.png)

![Record in the source's page](docs/capture-saisie.png)

## Development environment

The tools (pinned Rust, with rustfmt and clippy) and the project's commands are described in
[`mise.toml`](mise.toml), the exact versions in `mise.lock`. With [mise](https://mise.jdx.dev):

```sh
mise trust && mise install   # once: installs the project's toolchain
mise tasks                   # list of tasks
mise run app tree.ged        # launch the app
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

The CI ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)) runs the same tasks: `mise run
ci` on Linux, and `mise run bundle --dmg` on macOS (on `main` or on demand), whose `.dmg` can be
downloaded from the artifacts. mise-action's cache keeps the toolchain (rustup lives in mise's
folder), and a cache keyed on `Cargo.lock` keeps the crates and `target/`.

## Install

### macOS

```sh
mise run install
```

Builds GEDRoots in release mode, makes `GEDRoots.app` (icon, name in the menu bar and the Dock,
no Terminal) and copies it to `/Applications`. `.ged` files then open with a double-click or
with “Open With” in the Finder. Without `--install`, the app stays in
`target/release/GEDRoots.app` (`mise run bundle`); with `mise run bundle --dmg`, a
`GEDRoots.dmg` disk image is created next to it (useful for the CI).

The app is signed “ad hoc”, which is enough on the Mac that built it. A downloaded app (the
CI's `.dmg`) is blocked by Gatekeeper on first launch: right-click › Open, or
`xattr -dr com.apple.quarantine /Applications/GEDRoots.app`. To distribute it without this
warning, it would have to be signed with a Developer ID certificate and notarized by Apple
(paid Apple Developer account).

### Linux

```sh
mise run install
```

Installs `~/.local/bin/gedroots`, the icon and the applications menu entry.

### Windows

`mise run install` (or `cargo build --release -p gedroots`) produces
`target\release\gedroots.exe`, which opens without a console window.

The icon (`packaging/icon.png`) comes from the GEDRoots brand kit (dark app icon, placed on the
macOS grid).

## Run from source

```sh
mise run app path/to/tree.ged
```

Without an argument, the app offers to open a file or create a new one. Shortcuts: `⌘O` /
`Ctrl+O` open, `⌘S` / `Ctrl+S` save, `⌘N` new, `⌘E` enter a record, `⌘Z` / `Ctrl+Z` undo the
document's last change, `⌘⇧Z` / `Ctrl+Y` (Windows) / `Ctrl+Shift+Z` (Linux) redo it, `⌘[` /
`Alt+←` go back to the previous record.

Optional environment variables for the map: `GEDROOTS_OFFLINE=1` (start offline),
`GEDROOTS_TILE_URL` (another tile server, `{z}/{x}/{y}`), `GEDROOTS_TILE_CACHE` (cache folder,
by default `~/Library/Caches/GEDRoots/tiles` or `~/.cache/gedroots/tiles`),
`GEDROOTS_GEOCODER` (another Photon instance).

## Code layout

| Crate | Role |
| --- | --- |
| [`crates/gedcom`](crates/gedcom) | Lossless GEDCOM 7 reading/writing, typed views (people, families, sources, places, media), `CROP` cropping (`crop.rs`), multi-page media (`multipage.rs`), web links (`weblink.rs`), import assistant (`import.rs`), GEDZIP export preparation (`gedzip.rs`), GEDCOM 5.5.1 and Geneanet export (`gedcom551.rs`), online search sites (`websearch.rs`), editing forms, dates, coordinates (`geo.rs`), analysis tools (`analysis/`), Quality page checks (`quality.rs`), search (`search.rs`, `query.rs`, `phonetic.rs`, saved searches `saved.rs`), adding/linking relatives and media (`relations.rs`), descendants (`pedigree.rs`), source records (`act.rs`) and its own translations (`locales/`). |
| [`crates/gedroots`](crates/gedroots) | GPUI application: `commands.rs` (every command, shortcut and menu), `workspace.rs` (window and views) and `workspace/` (`commands.rs` commands and palette, `tree.rs` charts, `relatives.rs` relatives/picker/media, `add.rs` windows to add and edit events, `menus.rs` right-click menus, `photo.rs` cropping and people in photos, `pages.rs` multi-page media and merging, `weblinks.rs` web links, `import.rs` import assistant, `export.rs` GEDZIP and GEDCOM 5.5.1 exports, `filters.rs` quick filters, `quality.rs` Quality page, `insights.rs` Statistics page and relationship), `editor.rs` (visual and raw editors), `act_editor.rs` (record editor), `gedzip.rs` (writing the `.gdz` archive), `map.rs` (place map), `theme.rs` (design tokens), `locales/` (English and French texts). |

## Tests

```sh
mise run test
# Consistency check and statistics of a file, on the command line:
mise exec -- cargo run -p gedcom --example analyse -- ~/Genealogie/tree.ged
# Check that a real file is read and written back identically:
GEDROOTS_TEST_GED=~/Genealogie/tree.ged mise run test real_file -- --nocapture
```

The sample file `crates/gedcom/tests/fixtures/exemple.ged` is fictional. Personal `.ged` files
are ignored by git.

# GEDRoots GEDCOM extensions

GEDRoots stores its own data **in the `.ged` file**, never in a sidecar file (the version
history, a hidden folder beside the file, holds copies of the tree, not data of its own: see
[version-history.md](version-history.md)). All its tags
start with `_GR_` and are declared in the header, as GEDCOM 7 requires:

```
0 HEAD
1 SCHMA
2 TAG _GR_IGNORE https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_ignore
2 TAG _GR_ROOT https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_root
2 TAG _GR_TREE https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_tree
2 TAG _GR_NAME https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_name
2 TAG _GR_FILE https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_file
2 TAG _GR_EVEN https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_even
2 TAG _GR_MEDIA https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_media
2 TAG _GR_FIND https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_find
2 TAG _GR_WEB https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_web
2 TAG _GR_DISTINCT https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_distinct
2 TAG _GR_LEVEL https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_level
2 TAG _GR_TASK https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_task
2 TAG _GR_PROP https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_prop
2 TAG _GR_BOOK https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_book
```

Other software ignores these tags without losing data.

## `_GR_ROOT`

Root person of the tree: the one GEDRoots opens when the file loads and the one
**Person › Go to Root Person** brings you back to. Set in **File Settings › General**.

- **Under**: `HEAD`, at level 1. A single occurrence.
- **Value**: pointer to an `INDI` (`@I1@`).
- **Substructures**: none.

```
0 HEAD
1 SCHMA
2 TAG _GR_ROOT https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_root
1 _GR_ROOT @I00001@
```

Without this tag (or if it points to a deleted person), GEDRoots takes the Sosa 1 declared
by Ancestris (`_SOSADABOVILLE 1`), otherwise the first person in the file. Deleting the root
person also removes the tag.

## `_GR_TREE`

Identifier of the tree, so GEDRoots recognizes it after the file is renamed, moved or synced to
another computer.

- **Under**: `HEAD`.
- **Value**: a random UUID (version 4), written at the first save and never changed after.
- **Substructures**: none.

```
0 HEAD
1 SCHMA
2 TAG _GR_TREE https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_tree
1 _GR_TREE 3f2b8c1e-9a4d-4e57-b0c2-6d1f8e7a9b40
```

## `_GR_NAME`

Name of the tree on the computer that shares it, so a mobile device that follows several trees
can tell them apart. Written only in the copy sent to mobile devices, never in the `.ged`.

- **Under**: `HEAD`.
- **Value**: the tree's name, as its window shows it, without `.ged`.
- **Substructures**: none.

```
0 HEAD
1 SCHMA
2 TAG _GR_NAME https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_name
1 _GR_NAME Guyot
```

## `_GR_EVEN`

Custom event type (**GEDRoots › File Settings… › Events**). A custom type is a standard `EVEN`
with a `TYPE`: those already in the file are inferred from their `TYPE` and do not need this
tag. It is only used to offer a type not yet used in **Add Event…**, or to show a type under
a label other than its `TYPE`.

- **Under**: `HEAD`, at level 1. One occurrence per declared type.
- **Value**: the `TYPE` of the events of this type, as written in the file.
- **Substructures**:
  - `_GR_REC FAM` for a family event (under `FAM`); when absent, the type applies to
    individuals.
  - `_GR_LABEL <label>`: label GEDRoots shows instead of the `TYPE`. The events keep their
    `TYPE`.

```
0 HEAD
1 SCHMA
2 TAG _GR_EVEN https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_even
2 TAG _GR_REC https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_even
2 TAG _GR_LABEL https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_even
1 _GR_EVEN Baptême protestant
1 _GR_EVEN Publication des bans
2 _GR_REC FAM
1 _GR_EVEN military service
2 _GR_LABEL Service militaire
0 @I1@ INDI
1 EVEN
2 TYPE military service
2 DATE FROM 1914 TO 1918
```

Renaming a `TYPE` in the settings rewrites every matching `TYPE` (renaming to an existing
type merges them) and keeps its label; a declared, unused type can be removed. A declaration
that is no longer needed (type in use, no label) is removed.

On import (File › Open… on a file from other software), events with extension tags from other software
(`_MILT`, `_MILI`, `_DEG`, `_FUN`…, or any dated or located `_XXX` tag under an individual or
a family) become `EVEN` + `TYPE`.

## `_GR_MEDIA`

Filing of media files (**GEDRoots › File Settings… › Media**): the media folder, then
subfolders, each with the events it receives and its naming template. Paths stay standard
`FILE`s, relative to the `.ged` folder.

- **Under**: `HEAD`, at level 1, a single occurrence.
- **Value**: the media folder, relative to the `.ged` (may be empty).
- **Substructures**: one `_GR_DIR <subfolder>` per subfolder, in order:
  - `_GR_TYPES <list>`: what the folder receives, comma-separated: event tags (`BIRT`,
    `MARR`…), custom types (the `TYPE` of an `EVEN`), `INDI` (media attached directly to an
    individual: portrait) or `FAM`. When absent: everything else.
  - `_GR_NAME <template>`: file names. When absent: the original name is kept.

A media goes into the subfolder that names its type; if several name it, the most specific
one (fewest types) wins, then the first. Otherwise it goes into the first subfolder without
types; if no subfolder receives it, it stays where it is. In the settings, checking a type
for one subfolder removes it from the others.

```
0 HEAD
1 SCHMA
2 TAG _GR_MEDIA https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_media
2 TAG _GR_DIR https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_media
2 TAG _GR_TYPES https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_media
2 TAG _GR_NAME https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_media
1 _GR_MEDIA
2 _GR_DIR Matricules
3 _GR_TYPES military service, military discharge
3 _GR_NAME {sosa:6}_{branch}_{surname:simple}_{given}_{n}
2 _GR_DIR Actes
3 _GR_TYPES BIRT, BAPM, CHR, MARR, DEAT, BURI
3 _GR_NAME {sosa:6}_{branch}_{type:1}_{surname:simple}_{given}_{spouse:simple}_{n}
2 _GR_DIR Portraits
3 _GR_TYPES INDI
3 _GR_NAME {sosa:6}_{surname:simple}_{given}
2 _GR_DIR Divers
```

Template variables (`/` creates a subfolder):

| Variable | Value |
|---|---|
| `{type}` | event type ("Birth", or the `TYPE` of an `EVEN`), in the interface language |
| `{date}`, `{year}`, `{place}` | date (YYYY-MM-DD), year and town of the event |
| `{surname}`, `{given}`, `{given_names}` | surname, first given name, all given names; for a couple, the husband |
| `{spouse}` | for a couple, the wife's surname |
| `{sosa}`, `{branch}` | Sosa-d'Aboville number computed from the root person: `58` and `2` for the 2nd child of Sosa 58, `B1` for the 1st child of their 2nd union, `2.3` for a grandchild |
| `{source}`, `{page}`, `{title}` | source title, reference (`PAGE`), media title |
| `{n}` | page number of a media with several files (otherwise "p2", "p3"… at the end of the name) |

Options: `{sosa:6}` pads a number with zeros, `{type:1}` keeps the first letters (B, M,
D…), `{surname:simple}` removes accents, spaces and apostrophes. Variables are in English
whatever the interface language. An empty variable disappears along with the separator before it.

Variables are taken from the event that carries the media (`2 OBJE`), cites it, or cites a
source that carries it: preferably the one the source describes (`DATA.EVEN`) or its title
names ("Birth record…"), rather than an attribute (occupation, residence); on a tie, the one
the first subfolder receives. Otherwise from the source that carries the media, then from
the linked person or family. A file imported through **Record Media…** (⌘⇧M) is moved (or
copied, as chosen) under this name, the preview showing the name before import; a name
already taken gets "(2)", "(3)"… **Media › Rename Using Template…** moves the files of an
existing media after confirmation.

## `_GR_FILE`

Page of a multi-page media targeted by a link. A multi-page media is a standard `OBJE`
record with several `FILE`s, one per page, in order:

```
0 @M1@ OBJE
1 FILE Actes/acte_1.jpg
2 FORM image/jpeg
2 TITL Acte de mariage de Michel OHL et Jeanne BOUST
1 FILE Actes/acte_2.jpg
2 FORM image/jpeg
```

GEDCOM 7 applies a link's `CROP` to the first file only. To locate a person on another page
(a frame drawn on page 2 of a record), GEDRoots adds `_GR_FILE` to the link:

- **Under**: a link to a media (`n OBJE @M@`), next to `CROP`.
- **Value**: page number (of the `FILE`), starting at 1. Never written for page 1.
- **Substructures**: none.

```
0 @I1@ INDI
1 OBJE @M1@
2 CROP
3 TOP 120
3 LEFT 340
3 HEIGHT 260
3 WIDTH 200
2 _GR_FILE 2
```

A link with neither `CROP` nor `_GR_FILE` covers the whole document. Software that ignores
the extension applies the frame to the first page.

Merging media (**Media** menu, or ⌘ / Shift + click in the media list then right click),
adding, moving, removing or splitting a page (**Media › Pages**) updates `_GR_FILE` and every
pointer in the file, citations included.

## `_GR_IGNORE`

Hides an issue on the **Data Quality** page that the user has checked and found correct, for
instance a marriage really celebrated very young (right click on the issue › **Hide**;
**View › Filter List › Ignored Issues** shows them again, and **Stop Hiding** removes the
tag).

- **Under**: any record (`INDI`, `FAM`…), at level 1.
- **Value**: the issue code (see below). One line per hidden issue.
- **Substructures**: `NOTE` (optional): why the issue is correct, as written by an AI agent that
  proposed hiding it (`propose_change`) and shown on the Data Quality page.

```
0 @I263@ INDI
1 NAME Antoine /GIRAUDON/
1 _GR_IGNORE marriage_too_young
2 NOTE The marriage act gives her age as 14, with her parents' consent
```

Hidden issues are listed in **GEDRoots › File Settings…**, where they can be shown again (the tag
is removed, and its declaration too when none is left).

The issue is hidden when its main record (the one a click in the list opens) carries the tag
with the right code.

| Code | Issue |
| --- | --- |
| `death_before_birth` | Death before birth |
| `burial_before_death` | Burial before death |
| `event_before_birth` | Event before birth |
| `event_after_death` | Event after death |
| `too_old` | Lifespan over 110 years |
| `parent_too_young` | Parent under 14 at the child's birth |
| `father_too_old` | Father over 80 at the birth |
| `mother_too_old` | Mother over 55 at the birth |
| `birth_after_mother_death` | Birth after the mother's death |
| `birth_after_father_death` | Birth more than 10 months after the father's death |
| `child_before_parent` | Child born before their parent |
| `marriage_too_young` | Marriage before 14 |
| `marriage_after_death` | Marriage after a spouse's death |
| `marriage_age_gap` | More than 30 years between spouses |
| `siblings_too_close` | Two births (twins aside) less than 8 months apart |
| `husband_not_male` / `wife_not_female` | Sex inconsistent with the role in the family |
| `own_ancestor` | Person in their own ancestry |
| `broken_link` | Pointer to a missing record |
| `missing_backlink` | `FAMC`/`FAMS` without the matching `CHIL`/`HUSB`/`WIFE` |
| `unsourced` | Event without a source |
| `tag_not_allowed` | Standard tag not allowed here by the GEDCOM 7 schema |
| `missing_required` | Required substructure missing (e.g. a repository's `NAME`) |
| `too_many` | Substructure present several times where only one is allowed |
| `wrong_pointer` | Pointer to a record of the wrong type |
| `invalid_value` | Invalid value: unreadable or non-existent date, unknown enumerated value… |
| `empty_fact` | Event with no data at all (no value, no substructure) |
| `empty_note` | Empty note (`NOTE` without text, or empty shared note `SNOTE`) |
| `empty_family` | Family with no members |
| `unused_record` | Media, source, repository or shared note cited nowhere |
| `disconnected` | Individual (or group of individuals) with no family link or `ASSO` to the root person |
| `role_sex_mismatch` | `FATH`/`HUSB` role of a woman or `MOTH`/`WIFE` role of a man in a citation |
| `outside_source_period` | Record dated outside the period its source declares for that record type |
| `missing_file` | A media's file not found on disk |
| `moved_file` | File not found at its path, but exactly one file with the same name exists in the `.ged` folder |
| `duplicate_media` | Two media point to exactly the same files |
| `missing_form` | `FILE` without `FORM` (replaces `missing_required` for this case) |
| `unattached_file` | File in the `.ged` folder that no media points to (cannot be hidden: it has no record) |
| `missing_coords` | Place without coordinates (`PLAC.MAP`) |
| `bad_coords` | Unreadable or incomplete coordinates |
| `possible_duplicate_place` | Two places that may be the same: written differently (case, accents, "St"/"Saint", codes), a shortened form of exactly one other place, or a name that starts the other one ("Leudon" / "Leudon-en-Brie") under the same higher level with the same code or coordinates less than 2 km apart. Compared in the **Merge Two Places** window; hidden with `_GR_DISTINCT` in the header. An agent proposes to merge them (its choice for each level that differs) or to hide them, reviewed in To Review |
| `place_format` | Place whose name does not split into the jurisdictions of the place format (`HEAD.PLAC.FORM`): too few or too many commas. Located and completed in the place's record (**Place › Locate…** or **Complete…**), or by an agent |
| `place_hierarchy` | The same division attached to two higher divisions ("Loire" in "Rhône-Alpes" and in "Auvergne-Rhône-Alpes"); "Harmonize" keeps the most frequent |
| `possible_duplicate` | Two individuals that may be the same person; hidden with `_GR_DISTINCT` on the pair, not `_GR_IGNORE` |

File and place issues cannot be hidden: they concern no record where `_GR_IGNORE` could be
written. Possible duplicate places are the exception: **Not a Duplicate** writes the pair in the
header (see [`_GR_DISTINCT`](#_gr_distinct)).

The GEDCOM 7 schema check only applies to version 7 files; extension tags (`_XXX`) are never
reported.

Imprecise dates (`ABT`, `BEF`, `BET … AND …`, year only) are treated as ranges: an issue is
only reported if it holds for every possible date. "About" dates have a 5-year margin.

## `_GR_DISTINCT`

Two individuals that look alike but are different people: the user answered **Not a
Duplicate**, so the pair is no longer suggested among the possible duplicates, and an AI agent
cannot propose to merge them.

- **Under**: `INDI`, at level 1. One line per individual known to be someone else.
- **Value**: pointer to the other individual. Written on one of the two; GEDRoots reads it in
  both directions.
- **Substructures**: none.

```
0 @I710@ INDI
1 NAME Abraham /DARMON/
1 _GR_DISTINCT @I729@
```

Merging two individuals carries the marks of the merged-away one over to the kept one. If the
other individual is deleted, the line goes with it, like any pointer to a deleted record.

The same tag marks two **places** that look alike but are different (an old municipality and
the one it merged into, for instance). A GEDCOM place is not a record, so the mark goes in the
header:

- **Under**: `HEAD`, at level 1. One line per place, with one `PLAC` per place known to be
  another one.
- **Value**: the full place name, as in the `PLAC` tags of the file.
- **Substructures**: `PLAC <the other place>`, one or more. GEDRoots reads each pair in both
  directions.

```
0 HEAD
1 _GR_DISTINCT , Fontaine-Denis, 51120, Marne, Grand Est, France
2 PLAC , Fontaine-Denis-Nuisy, 51120, Marne, Grand Est, France
```

Renaming or merging a place updates its marks; a pair that becomes a single place loses its
mark.

## `_GR_LEVEL`

What each jurisdiction of the place format (`HEAD.PLAC.FORM`) is, for OpenStreetMap: GEDRoots
fills it from the gazetteer when a place is created, located or completed. A jurisdiction is
recognized by its name (“Commune”, “Département”, “Code_Postal”, “County”, “Gemeinde”,
“Provincia”… in six languages), or by the tree's values when its name says nothing (postal
codes, country names); otherwise it is left empty. `_GR_LEVEL` keeps only what the user
corrected in **File Settings › Places**, and is absent when nothing was.

- **Under**: `HEAD.PLAC`, at level 2, beside `FORM`.
- **Value**: one keyword per jurisdiction, in the order of `FORM`, separated by commas: `locality`,
  `municipality`, `postcode`, `county`, `state`, `country`, `none` (never filled), or nothing
  (recognized automatically).
- **Substructures**: none.

```
0 HEAD
1 PLAC
2 FORM Paroisse, Code INSEE, Niveau 3, Pays
2 _GR_LEVEL , none, county,
```

## Source and media types (no extension)

GEDRoots types sources and media with GEDCOM 7's standard structures, which other software
reads:

| Data | GEDCOM written |
| --- | --- |
| Content of a source: record types, period (source record, **Records** and **Period** fields) | `SOUR` / `DATA` / `EVEN BIRT, DEAT` + `DATE FROM 1793 TO 1802` (+ `PLAC`) |
| Medium of a media (media record, **Medium** list) | `OBJE` / `FILE` / `FORM` / `MEDI PHOTO`, `MANUSCRIPT`, `TOMBSTONE`… |
| Portrait of an individual (right click on an image › **Set as Portrait**) | their `OBJE` link moves to the top of their media; the media becomes `MEDI PHOTO` if it had no medium |

Entering a record adds its type to the source's content and widens the period to the
record's year; the image of an entered record is a `MANUSCRIPT`.

Portrait shown: the individual's first photo (`MEDI PHOTO`), otherwise an image without a
medium filed in a "Portraits" folder or titled "Portrait", otherwise an image without a
medium that is not a source scan, otherwise their first image.

The import wizard converts `_PRIM Y` (main photo in Family Tree Maker, Legacy,
MyHeritage…): the marked link moves to the top of the media and the tag is removed.

## Source records (no extension)

A record (a civil or church act) is entered in its source's page ("+" next to **Records**,
or **Enter Record…**, `⌘E` / `Ctrl+E`). There is no template: you pick the event, then the
people with their GEDCOM 7 role. Saving writes only standard structures:

| Data entered | GEDCOM written |
| --- | --- |
| Event of the record | any event: `BIRT`, `BAPM`, `DEAT`, `CENS`… on each principal person, `MARR`, `DIV`… on the spouses' family, `EVEN` + `TYPE` |
| Reference (record number, image…) | `SOUR @S@` / `PAGE` |
| Record date, transcription | `SOUR` / `DATA` / `DATE`, `TEXT` |
| Role of each person | `SOUR` / `EVEN <tag>` / `ROLE CHIL, FATH, MOTH, HUSB, WIFE, SPOU, WITN, GODP, OFFICIATOR, CLERGY, NGHBR, FRIEND, OTHER` (+ `PHRASE`: "Godfather", "Bride's father", stated relationship) |
| Witnesses, godparents, informants… | `ASSO @I@` / `ROLE` on the main event |
| Stated age | `AGE` (`32y`), `HUSB`/`WIFE` / `AGE` for a marriage; estimated birth `BIRT` / `DATE EST 1880` |
| Occupation, residence, stated birthplace | dated `OCCU`, `RESI` and `BIRT` / `PLAC`, each sourced |
| Quality | `QUAY` 0 to 3 |
| Record image | `OBJE` linked to the source and to each citation |
| Person only mentioned | `NOTE` of the main citation (`Witness: Jean DUPONT, 32 years old`) |

Family links follow from the roles: `FATH` / `MOTH` of a principal person attach them to
their family, `SPOU` forms a couple, and in a household (several principal people, e.g. a
census) the stated relationship ("wife", "son"…) links each person to the first.

**List of a source's records.** GEDRoots rebuilds it from the citations of individuals and
families that point to the source, grouped by `PAGE` and `DATA.DATE`: the principal person
is the one whose event carries the citation, the others are read from their `ROLE`.
Citations written by other software therefore show up too. Reopening a record replaces its
citations; deleting it removes its citations and its `ASSO`s, leaving the events and the
individuals.

The texts GEDRoots writes into the file (`PHRASE`, mention notes, record titles) follow the
interface language.

## `_GR_FIND`

Saved search, shown in the **Searches** section of the sidebar with its number of results.
Saved with the section's "+"; renamed (double click, right click
or **View › Rename Search**) and deleted the same way.

- **Under**: `HEAD`, at level 1. One occurrence per search, in sidebar order.
- **Value**: name of the search.
- **Substructures**:
  - `_GR_SCOPE <scope>` (required): `INDI`, `SOUR`, `PLAC` or `OBJE`;
  - `_GR_QUERY <query>`: criteria and words, in the search field's syntax
    (`born:1850±5 sex:m`, `ancestors:@I1@`), with English keywords;
  - `_GR_TYPE <type>`: media type chosen in the Media area (subfolder from the settings or
    the file).

```
0 HEAD
1 SCHMA
2 TAG _GR_FIND https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_find
2 TAG _GR_SCOPE https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_find
2 TAG _GR_QUERY https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_find
2 TAG _GR_TYPE https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_find
1 _GR_FIND Born around 1850
2 _GR_SCOPE INDI
2 _GR_QUERY born:1850±5
1 _GR_FIND Unattached records
2 _GR_SCOPE OBJE
2 _GR_QUERY without:link
2 _GR_TYPE Actes
```

Deleting the last search also removes the declarations.

## `_GR_WEB`

Online search site, offered in **Person › Search On** and when right-clicking a person. Set
in **GEDRoots › File Settings… › Search**.

- **Under**: `HEAD`, at level 1. One occurrence per site, in menu order.
- **Value**: name of the site.
- **Substructures**: `_GR_URL <template>`: address of the search, where each variable in
  braces is replaced by the person's data, URL-encoded (empty if unknown): `{surname}`,
  `{given}`, `{birth}` and `{death}` (years), `{place}` (town of birth),
  `{father_surname}`, `{father_given}`, `{mother_surname}`, `{mother_given}`,
  `{spouse_surname}`, `{spouse_given}`.

Without `_GR_WEB`, GEDRoots offers Geneanet and FamilySearch. A lone `_GR_WEB`, with no value
and no `_GR_URL`, means no site is wanted.

```
0 HEAD
1 SCHMA
2 TAG _GR_WEB https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_web
2 TAG _GR_URL https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_web
1 _GR_WEB Geneanet
2 _GR_URL https://www.geneanet.org/fonds/individus/?go=1&nom={surname}&prenom={given}&type_periode=between&from={birth}&to={death}
1 _GR_WEB Google
2 _GR_URL https://www.google.com/search?q=%22{given}%20{surname}%22%20{place}
```

## `_GR_TASK`

Research log of a record still to find (a *lead*: birth, marriage, death, parents): whether it is
planned, ignored or found, every search made, including those that found nothing, the records that
may be the one but are not proven yet (*clues*), and where the work stands (*handoffs*). It is the
memory AI agents share: any agent takes over another's work from the file alone, and nobody
searches the same register twice. Written by the agents and by the Research page. The leads
themselves are computed, not stored.

- **Under**: the `INDI` concerned, or the `FAM` for a marriage, at level 1. One per lead.
- **Value**: the record to find: `BIRT` (birth or baptism), `MARR`, `DEAT` (death or burial) or
  `FAMC` (the parents of a person whose parents are unknown).
- **Substructures**:
  - `_GR_STAT <state>`: `PLANNED`, `NOT_FOUND` (searched in vain for now), `FOUND`, `IGNORED`;
    absent: only suggested;
  - `NOTE <text>`: why it is planned or ignored;
  - `_GR_LOG`, one per entry, oldest first:
    - `DATE <date>` with `TIME <time>` (UTC): when it was noted;
    - `_GR_KIND <kind>`: `CLUE` or `HANDOFF`; absent: a search;
    - `TEXT <text>`: a search: what was searched (archive, register or index, years, names
      tried); a clue: the record found; a handoff: where the lead stands;
    - `_GR_RES <result>`, for a search: `FOUND`, `NONE` or `PARTIAL` (part of what was looked
      for, such as an index entry without the act);
    - `_GR_SPAN <years>`: years searched (`1853`, `FROM 1853 TO 1862`);
    - `PLAC <place>`, `WWW <url>` (page of the archive's viewer), `NOTE <text>`;
    - `_GR_FOR <text>`, `_GR_AGNST <text>`, for a clue: what speaks for it being this person, and
      against;
    - `_GR_NEXT <text>`, for a handoff: the next search to make;
    - `_GR_AGENT <name>`: noted by an AI agent (its name, such as `Claude Code`); absent: by the
      user.

```
0 HEAD
1 SCHMA
2 TAG _GR_TASK https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_task
2 TAG _GR_STAT https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_task
2 TAG _GR_LOG https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_task
2 TAG _GR_RES https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_task
2 TAG _GR_SPAN https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_task
2 TAG _GR_AGENT https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_task
2 TAG _GR_KIND https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_task
2 TAG _GR_FOR https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_task
2 TAG _GR_AGNST https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_task
2 TAG _GR_NEXT https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_task
0 @I4@ INDI
1 _GR_TASK DEAT
2 _GR_STAT PLANNED
2 NOTE Last seen in 1852, his widow remarried in 1860
2 _GR_LOG
3 DATE 3 NOV 2026
4 TIME 20:14:52
3 TEXT Decennial tables of Saint-Lô, deaths, MARCHAND and MARCHANT
3 _GR_RES NONE
3 _GR_SPAN FROM 1853 TO 1862
3 PLAC Saint-Lô, Manche, France
3 _GR_AGENT Claude Code
2 _GR_LOG
3 DATE 3 NOV 2026
4 TIME 20:31:07
3 _GR_KIND CLUE
3 TEXT Burial of Jean MARCHAND, 4 May 1858, Carentan
3 WWW https://archives.manche.fr/ark:/…
3 _GR_FOR Same name, widower of a Rose
3 _GR_AGNST Aged 50, about ten years younger than our Jean
3 _GR_AGENT Claude Code
2 _GR_LOG
3 DATE 3 NOV 2026
4 TIME 20:40:12
3 _GR_KIND HANDOFF
3 TEXT Saint-Lô tables searched 1853–1862; one clue in Carentan
3 _GR_NEXT Carentan deaths 1858: age and spouse's name
3 _GR_AGENT Claude Code
```

Legacy and RootsMagic keep their to-do items in `_TODO`; GEDRoots does not read or write it.

## `_GR_PROP`

A proposal of an AI agent (a record found in the archives, a correction, a book chapter): kept
while it waits in To Review, so that it survives a restart and other agents see it, then with what
the user did with it, so that any agent knows what was saved, rejected or corrected without having
to remember it.

- **Where**: a record at level 0, with an identifier (`@GRP3@`). One per proposal.
- **Value**: the proposal's id, as the agents know it (`P3`).
- **Substructures**:
  - `_GR_KIND <kind>`: `ACT`, `CHANGE` or `CHAPTER` (`_GR_KIND`, `_GR_RES` and `_GR_AGENT` are
    the research log's tags, declared with its URI);
  - `DATE <date>` with `TIME <time>` (UTC): when it was sent;
  - `TEXT <text>`: what it is, in one line;
  - `_GR_ABOUT @<XREF>@`: the records it is about (people, families, sources…), one per line; places, which are not records, are never listed;
  - `_GR_RES <outcome>`: `WAITING`, `SAVED`, `REJECTED`, `EDITED` (opened in record entry by the
    user), `WITHDRAWN` or `REPLACED`;
  - `_GR_AGENT <name>`: the agent that sent it;
  - `NOTE <text>`: the user's reason, or the proposal that replaced it;
  - `_GR_DATA <json>`: the agent's request as it sent it, only while it waits: GEDRoots queues it
    again when the file opens.

```
0 HEAD
1 SCHMA
2 TAG _GR_PROP https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_prop
2 TAG _GR_ABOUT https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_prop
2 TAG _GR_DATA https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_prop
0 @GRP3@ _GR_PROP P3
1 _GR_KIND ACT
1 DATE 5 NOV 2026
2 TIME 21:47:03
1 TEXT Marriage · Johann SCHANDL & Maria RIEDAUER
1 _GR_ABOUT @I12@
1 _GR_RES REJECTED
1 _GR_AGENT Claude Code
1 NOTE Not our Johann: he was born about 1726
```

## `_GR_BOOK`

A family book (Create on the Books page): a PDF typeset from the tree, with its charts, a record for each
person written from the facts, and an index. A book longer than 500 pages is exported in volumes,
cut between chapters. It is written in the tree's language (`HEAD.LANG`) when GEDRoots is
translated into it, otherwise in the interface's. Only what the book is made of is stored; its
pages are computed from the tree each time, so the book follows the changes. See
`crates/book`.

- **Where**: a record at level 0, with an identifier (`@BK1@`). One per book.
- **Value**: the title; when empty, GEDRoots shows “The *surname* family”.
- **Substructures**:
  - `_GR_SUBJ @<INDI>@`: the person the book starts from;
  - `_GR_LINE <line>`: `ANCESTORS` (the person's ancestors, one chapter per generation) or
    `DESCENDANTS`; absent: `ANCESTORS`;
  - `_GR_GENS <n>`: generations, from 2 to 12; absent: 8;
  - `_GR_SIZE <size>`: page size, `170X240` (17 × 24 cm, a printed book), `156X234`
    (15.6 × 23.4 cm, Lulu's “Royal”), `152X229` (6 × 9 in, Lulu and Blurb), `A4` or `LETTER`;
    absent: `170X240`;
  - `_GR_PRNT Y`: exported for a printer: the pages with 3.175 mm of bleed, a title page in
    place of the cover, and the cover as a file of its own (back, spine, front); absent: for
    reading and printing at home.
  - `_GR_CHAP <n>`: what an AI agent wrote for chapter *n* (the generation, 1 = the person),
    saved from To Review, and the visualizations placed in it (by the agent, or with Add to Book
    on the Visualizations page). Its substructures, in the order the book shows them:
    - the chapter's introduction, as blocks;
    - `_GR_INDI @<INDI>@`: a person's narrative, as blocks, in place of the sentences GEDRoots
      writes from the facts.

    A block is one of:
    - `_GR_TEXT <text>` (with `CONT`): facts told as a narrative; `{I12}` stands for a person,
      shown by name;
    - `_GR_CTXT <title>`: historical context, with `_GR_TEXT` and one `_GR_CITE <source>` or more
      (author and title), each with an optional `_GR_URL <url>`;
    - `_GR_HYPO <text>` (with `CONT`): what the records do not say, with one `_GR_BASE <clue>` or
      more;
    - `_GR_VIZ <kind>`: a visualization, drawn from the tree when the book is typeset: `FAN`,
      `PLACES`, `LIVES`, `GENERATIONS`, `LONGEVITY` or `OCCUPATIONS`. By default it shows the
      book's people; `_GR_SUBJ @<INDI>@`, `_GR_LINE` and `_GR_GENS` give another scope. An
      optional `_GR_TEXT` is a comment shown under it. One of the book's own people is shown
      there instead of in the book's opening pages or figures; one the tree has too little for
      is left out.

```
0 HEAD
1 SCHMA
2 TAG _GR_BOOK https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_book
2 TAG _GR_SUBJ https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_book
2 TAG _GR_LINE https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_book
2 TAG _GR_GENS https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_book
2 TAG _GR_SIZE https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_book
2 TAG _GR_PRNT https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_book
0 @BK1@ _GR_BOOK The Guyot Family
1 _GR_SUBJ @I1@
1 _GR_LINE ANCESTORS
1 _GR_GENS 8
1 _GR_SIZE 170X240
1 _GR_CHAP 3
2 _GR_CTXT The phylloxera crisis
3 _GR_TEXT From 1863, an aphid destroyed the vineyards of France…
3 _GR_CITE Gilbert Garrier, Le Phylloxéra, une guerre de trente ans
2 _GR_INDI @I6@
3 _GR_TEXT {I6} is born in 1864 in Troyes, the son of a winegrower…
3 _GR_HYPO He left the vines for the railway when the phylloxera reached the Aube.
4 _GR_BASE occupation changes between 1888 and 1895
2 _GR_VIZ PLACES
3 _GR_TEXT Every branch of this generation still lives within a day's walk of Troyes.
```

Deleting the person leaves the book without a subject: it stays in the list, empty, until it is
deleted. GEDCOM 5.5.1 exports leave it out.

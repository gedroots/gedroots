# Version history

Every save is kept in a hidden folder beside the `.ged` file, so that it follows the file through
Dropbox, iCloud Drive or OneDrive. For `Guyot.ged`, the folder is `.Guyot.ged.history/` (hidden by
its dot on macOS and Linux, by the hidden attribute on Windows). The code is
`crates/gedcom/src/versions.rs`.

## When a version is recorded

- when a file is opened: its first version, or the changes made outside GEDRoots since the
  latest one (nothing when it did not change);
- at each save.

Later: named versions, accepted AI agent proposals and restore points before bulk operations
(the kinds exist already).

## Files

```
.Guyot.ged.history/
  versions/<id>.ver
  packs/<id>.pack
```

`<id>` is the time in milliseconds since 1970 (14 digits, always later than the latest version)
followed by 12 random hexadecimal digits: versions sort by it, and two computers never choose
the same one.

**Every file is written once and never modified** (through a temporary `.tmp` file renamed). Two
computers that write at the same time only add files: a sync service never makes a "conflicted
copy", and the union of both folders is a valid history. Only the retention deletes files.

### Version (`.ver`)

Text compressed with deflate (raw, no zlib header), one field per line:

```
GEDROOTS VERSION 1
id 01793724394512-3f2b8c1e9a4d
time 1793724394
kind saved
label Before merging duplicates
author Claude
tree 3f2b8c1e-9a4d-4e57-b0c2-6d1f8e7a9b40
added 2
changed 1
removed 0
head <hash>
rec <hash> I00001
rec <hash> F00001
```

- `kind`: `opened`, `saved`, `named`, `agent`, `restore-point`, `outside`, `restored`.
- `label`, `author`, `tree` (`HEAD._GR_TREE`) are optional.
- `added`, `changed`, `removed`: records compared with the previous version, by identifier.
- `head`: the header; `rec`: the records in file order, with their identifier when they have one.
- A hash is the BLAKE3 hash of the record's GEDCOM lines (`\n` line endings), first 128 bits in
  hexadecimal.
- Unknown lines are ignored, so that a later version of GEDRoots can add some.

### Pack (`.pack`)

The records a version brought that no earlier pack holds: an unchanged record is stored once.

```
GEDROOTS PACK 1
<hash> <length in bytes>
<hash> <length in bytes>

<deflate of the records' lines, one after the other>
```

The index is plain text so that GEDRoots knows which records are stored without inflating the
data.

## Retention

After each save, GEDRoots keeps:

- every version of the last 24 hours;
- the last version of each day for 30 days, then the last one of each week;
- named versions, until deleted;
- restore points, agent and outside versions for 30 days;
- the latest version.

Then it deletes the packs no version needs, and copies the needed part of a pack that is mostly
unused into a new pack before deleting it. Days and weeks are counted in UTC.

## Renamed and copied files

When a file was renamed or moved within its folder, GEDRoots finds the history it left (same
`_GR_TREE`, file gone) and renames the folder to follow it. A copy of a file that still exists
starts its own history.

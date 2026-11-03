# Version history

Every save is kept in a hidden folder beside the `.ged` file, so that it follows the file through
Dropbox, iCloud Drive or OneDrive. For `Guyot.ged`, the folder is `.Guyot.ged.history/` (hidden by
its dot on macOS and Linux, by the hidden attribute on Windows). The code is
`crates/gedcom/src/versions.rs`.

## When a version is recorded

- when a file is opened: its first version, or the changes made outside GEDRoots since the
  latest one (nothing when it did not change);
- with File › Save, and when the window closes or the app quits;
- while editing: the file is saved automatically about a second after the last change (and when
  the window loses focus), but a version is recorded at most every 5 minutes;
- when the open file is changed outside GEDRoots: GEDRoots reloads it when it has no changes of
  its own, otherwise it keeps its changes and records the other copy as an `outside` version.

New trees and imported trees are saved automatically only once File › Save has given them a
file.

- with File › Save Version…: a `named` version, with the name given, recorded even when nothing
  changed since the latest version;
- when an earlier version is brought back (File › Revert To): a `restored` version, after the
  changes not recorded yet have become a version of their own.

- when a proposal of an AI agent is accepted in To Review: an `agent` version of its own, saved
  at once, labelled with what it did and authored by the agent (the changes not recorded yet
  become a version first, so that the agent's version holds only its changes);
- before an operation that changes many records (merging two people, Quality › Fix All, the
  import wizard's conversions): a `restore-point` version, labelled with the operation, recorded
  even when nothing changed since the latest version.

## Browsing and restoring

File › Revert To › Browse All Versions… (`crates/gedroots/src/workspace/version_history.rs`)
lists the versions newest first, grouped by day in local time, with the records each one added,
changed and removed since the one before it (`Version::changes_since`). The oldest version kept
only shows its number of records.

Restore This Version and Revert To › Last Opened never overwrite anything: the tree as it was
becomes the current tree as one undo point, is saved, and is recorded as a `restored` version.
Save a Copy… writes the selected version to a separate `.ged`.

Undo This Change (`Store::undo_changes`) undoes only what the selected version changed: each
record it added, changed or removed goes back to how it was in the version before, unless it was
changed again since (its current hash differs from its hash in the selected version), in which
case it is left as it is and counted. The result is a `restored` version labelled `undo <id>`.

## Record history

Edit › Record History… (`crates/gedroots/src/workspace/record_history.rs`) lists the versions
that changed the selected record (`Store::record_history`), newest first, each with the fields
that changed, before and after, in the genealogist's words (`crates/gedcom/src/record_diff.rs`:
facts matched by tag and `TYPE`, a fact that only moved and the change dates `CHAN`/`CREA` are not
changes). Restore This State brings the record back as it was after the selected change, as one
undo point.

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
- `label`, `author`, `tree` (`HEAD._GR_TREE`) are optional. The `label` of a `named` version is
  its name; that of a `restored` version is the id of the version it brought back, `opened` for
  the tree as it was opened, or `undo <id>` for the version whose changes were undone; that of an
  `agent` version or a `restore-point` says what was done. `author` is the AI agent's name.
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

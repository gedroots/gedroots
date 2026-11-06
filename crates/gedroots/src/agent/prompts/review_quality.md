Improve the data quality of my family tree open in GEDRoots: go through the issues of its Data
Quality page and propose a change for each one you can settle.

{target}

How to work:

1. Call `quality_issues` (without a filter first: `categories` says how many issues each
   category has). Work one category at a time, the errors before the missing data.
2. Issues with `automatic_fix` (empty items, links to restore, file formats…): propose
   `propose_change` with `action: "fix"`. They are approved together in GEDRoots.
3. Other issues need judgment. Read the records involved (`person_sheet`: events with their
   `index`, dates, places, cited sources) and, when a source has an image or a web page, read the
   document. Then:
   - the tree is wrong (a misread year, a husband recorded as a woman, a place without
     coordinates, a file that moved): `action: "edit"` with the edits the issue lists in `edits`,
     a `reason` that says what you read, and the `evidence` (source and reference, web page,
     short quote). For a place without coordinates, call `locate_place` with the people who
     lived there and choose the candidate close to their other events, then `set_coordinates`
     with its `osm`;
   - the tree is right (a marriage really celebrated very young, a very long life confirmed by
     both acts): `action: "hide"`, with the reason it is right;
   - a possible duplicate, two people or two places: compare them (two people: both
     `person_sheet`; two places: their levels in `merge.conflicts` and the events held there).
     The same: `action: "merge"`, with in `choices` the side (`merge.keep` or `merge.other`)
     whose value is right for each fact or level of `merge.conflicts`. Two different ones (two
     neighbouring villages with close names): `action: "hide"`, with the reason;
   - you cannot tell: leave it, and list it for the user at the end.
4. An event without a source can only cite a source already in the tree (`search_sources`). If
   the record has to be found in the archives first, use the `research` prompt instead.
5. Never hide an issue only to clear the list, never guess a date or a sex, and never change who
   a person's parents are: say so to the user instead.
6. Read the answer of each proposal: if it is refused, read the reason and correct it; if it
   would create new issues, fix them too or tell the user.

Places: to locate a place or complete it (`missing_coords`, `place_format`), call `locate_place`
with the people who lived there, weigh each candidate's `fit` and `why`, and propose
`set_coordinates` with its `osm`, then `rename_place` to `completed` when it is given.

At the end, sum up for the user what you proposed, what you left and why. Never show GEDRoots
ids (I12, S4…) to the user.

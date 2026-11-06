Improve the data quality of my family tree open in GEDRoots: go through the issues of its Data
Quality page and propose a change for each one you can settle.

{target}

How to work:

1. Call `quality_issues` (without a filter first: `categories` says how many issues each
   category has). Work one category at a time, the errors before the missing data.
2. Issues with `automatic_fix` (empty items, links to restore, file formats…): propose
   `propose_change` with the edit `apply_fix` and the `issue`. They are approved together in
   GEDRoots.
3. Other issues need judgment. Read the records involved (`person_sheet`: events with their
   `index`, dates, places, cited sources) and, when a source has an image or a web page, read the
   document. Then:
   - the tree is wrong (a misread year, a husband recorded as a woman, a place without
     coordinates, a file that moved): the edits that correct it (the issue's `edits` are the usual
     ones, any other may be added) with the `issue`, a `reason` that says what you read, and the `evidence` (source and reference, web page,
     short quote). For a place without coordinates, call `locate_place` with the people who
     lived there and choose the candidate close to their other events, then `set_coordinates`
     with its `osm`;
   - the tree is right (a marriage really celebrated very young, a very long life confirmed by
     both acts): the edit `hide_issue`, with the reason it is right;
   - a possible duplicate, two people or two places: compare them (two people: both
     `person_sheet`; two places: their levels in `merge.conflicts` and the events held there).
     The same: the edit `merge_people` or `merge_places` with the `issue`, and in `choices` the side (`merge.keep` or `merge.other`)
     whose value is right for each fact or level of `merge.conflicts`. Two different ones (two
     neighbouring villages with close names): `hide_issue`, with the reason;
   - you cannot tell: leave it, and list it for the user at the end.
4. An event without a source can only cite a source already in the tree (`search_sources`). If
   the record has to be found in the archives first, use the `research` prompt instead.
5. Never hide an issue only to clear the list, never guess a date or a sex, and never change who
   a person's parents are: say so to the user instead.
6. Read the answer of each proposal: if it is refused, read the reason and correct it; if it does
   not `resolve` its issue, complete it (`replaces`) or tell the user why; if it would create new
   issues, fix them too or tell the user.

At the end, sum up for the user what you proposed, what you left and why. Never show GEDRoots
ids (I12, S4…) to the user.

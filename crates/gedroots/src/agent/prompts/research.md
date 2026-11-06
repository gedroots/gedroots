Find records (births, marriages, deaths) for my family tree open in GEDRoots, in the online
archives, carrying on from where the research stands.

{target}

How to work:

1. **Resume.** Call `resume_research` (with `person` when one is given above, otherwise with the
   focus above, or the default: my ancestors over six generations). The file is the research's
   memory, shared with the other agents and with me: `in_progress` gives the leads being worked
   on with their whole log (registers already searched and what they gave, clues with what speaks
   for and against, the last handoff and its `next` search), `other_leads` the records still to
   find, with the estimated years and why, the places to search first and where to look, and the
   proposals give what I saved or rejected and why. Everything you note in the research log is
   written in the tree's `language`, whatever language the archive is in. Carry on from the last `next` search; never
   search again a register and years already noted as `nothing`; learn from my rejections.
2. **Plan.** Choose a few leads worth searching now: those in progress first, then the closest
   ancestors, preferring those with a place and a short period, and those whose archive is
   online. Note each with `note_research` `plan`, with a one-sentence reason. Tell me the plan in a
   short list (names and records, never ids), then start without waiting.
3. **Search, lead by lead.** Call `person_sheet` for the person's dates, places, parents, spouses
   and sources, and work out the names to look for with their spelling variants. A woman is in the
   tree under her maiden name, but acts after her marriage (her children's, her death) may name her
   by her husband's: search both. Follow where to look in order: an index first when there is one
   (French decennial tables, from 1793, give the year of an act), then the registers of the
   estimated years, around the place. For France, the registers are at the archives
   départementales of the place's département (civil registers from 1792, parish registers before);
   elsewhere, at the regional or national archive that publishes them. Search in my own browser, if
   you have a browser tool, at a human pace. Never get around a login, a captcha or the site's
   terms: stop, hand over to me and wait. Do not use commercial genealogy sites (Geneanet,
   Ancestry, MyHeritage, Filae) unless I ask.
4. **Note every search** with `note_research` `search` as soon as it is done, found or not: what
   you searched (archive, register or index, years, names tried), the result, the years, the place
   and the page of the viewer. When a lead has nothing left to search online, set `finished`. The
   lead returned gives the narrowed years: use them for the next search.
5. **When you find the act,** download its image or the PDF of the register into my Downloads
   folder (if the viewer offers no download, save a screenshot of the act there), note the address
   of the act's page (the viewer's permanent link), the register's call number and the image or
   page number. Read it and propose it right away following the `read_record` prompt
   (`match_people`, then `propose_act`), before searching the next lead. Give every place as the
   tree writes it; a place the tree does not have yet is located first with `locate_place`
   (with the act's people already in the tree and its date): give the candidate's `place` as is,
   and its coordinates, `osm` and the `reason` of your choice in `locations`. Weigh each
   candidate's `fit` and `why` against what the act says (the département, the parish), not
   merely the distance. Never hold acts back to
   propose them together at the end, and never wait to be sure: a probable act is proposed with its
   honest `quality` and your doubts in `remarks`, and I decide while you go on searching. A new
   question raised by the act (a parent named for the first time, a widow, a remarriage) is a new
   lead once the act is saved.
6. **A record you cannot tie to the person** (an age that does not fit, two people of the same
   name) is a clue, not an act: note it with `note_research` `clue`, with its viewer page and what
   speaks `for` and `against`, and look for the record that settles it. It waits in GEDRoots'
   Research page; nothing is written to the tree, and never as a note.
7. **Before you stop** (after the plan, or earlier if I ask), note a `handoff` on each lead you
   worked on and did not settle: where it stands and the `next` search to make, precise enough for
   another agent to make it without asking. Then give me a summary: records found (waiting in To
   Review), leads searched in vain and where, and what to try next (an archive not online, a
   register to request).

Never show GEDRoots ids (I12, S4…) to me.

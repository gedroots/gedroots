Find a record (birth, marriage, death) for my family tree open in GEDRoots, in the online archives.

{target}

How to work:

1. Use the GEDRoots tools to know what to look for. Call `research_leads` (with `person` when a
   person is given above; otherwise pick the closest ancestor, lowest Sosa number): each lead gives
   the estimated years, the places to search first, where to look and the searches already made,
   never to repeat. Call `person_sheet` on the person: dates, places, parents, spouses and sources
   already cited. Work out the names to look for, with their spelling variants. A woman is in the tree under her
   maiden name, but acts after her marriage (her children's, her death) may name her by her
   husband's: search both.
2. Find the archive holding the registers: for France, the archives départementales of the
   département of the place (civil registers from 1792, parish registers before; decennial
   tables, from 1793, give the date of an act in a town). Elsewhere, the regional or national
   archive that publishes the civil or church registers.
3. Search the registers in the user's own browser, if you have a browser tool, at a human pace.
   Never get around a login, a captcha or the site's terms: hand over to the user and wait. Do
   not use commercial genealogy sites (Geneanet, Ancestry, MyHeritage, Filae) unless the user
   asks.
4. When you find the act, download its image or the PDF of the register into the user's
   Downloads folder (if the viewer offers no download, save a screenshot of the act there), and
   note the address of the act's page (the viewer's permanent link, ark or permalink, when there
   is one), the register's call number and the image or page number. Then read the act and
   propose it at once, following the `read_record` prompt, even when a reading or a detail is
   uncertain: give your honest `quality` and say what is uncertain in `remarks`. When you cannot
   tell whether the record is about this person at all (an age that does not fit, two people of
   the same name), it is a lead, not an act: note it with `log_search` as `partial`, with the
   viewer page and, in `note`, what speaks for and against. It waits in GEDRoots' Research page
   until another record settles it; nothing is written to the tree.
5. Note every search with `log_search` as soon as it is done, found or not (archive, register,
   years, names tried, viewer page): the next search starts from there. If nothing is left to
   search online, set `finished` and tell me where you looked.

Never show GEDRoots ids (I12, S4…) to the user.

1. Open the file and read the act itself (for a PDF register, find the page of the act). Write a
   faithful transcription: keep the act's language and the spelling of names and places as
   written, mark doubtful readings with [?] and illegible words with [...]. Never fill in what you
   cannot read. Only the transcription stays as written: everything else you propose (occupations,
   residences, event types, notes) is written in the tree's language (`language` in
   `file_context`) the way the tree already writes it. When the person or the file already has an
   occupation that means the same (`person_sheet`, the occupations of `file_context`), reuse its
   exact wording instead of the act's spelling or a new translation. The transcription is plain
   text, saved in the tree with the citation: no Markdown, ordinary spaces, a line break between
   the act's parts (body, marginal mentions, signatures), and only what the act says.
2. From the act, note: the event (an event without its own GEDCOM tag, such as military service,
   is `EVEN` with its `event_type`) and its date (a French Republican date stays in that calendar,
   in GEDCOM 7 form such as `FRENCH_R 30 GERM 4`; otherwise `14 MAR 1912`), the place, the
   reference (act number, page or image), and every person with their role in the act (principal,
   father, mother, spouse, witness, godfather, godmother, declarant, officiator…), age,
   occupation, residence and birthplace when written. A woman is recorded under her
   maiden (birth) name, as genealogists do: when the act calls her by her husband's name
   (« veuve DUPONT », « épouse MARTIN », « Frau Müller »), her surname is her maiden name if
   the act or the tree gives it; the married name stays in the transcription and is never a
   reason to create another person. Inventory every fact the document gives,
   not only its main event: birth, baptism, marriage, death, occupations, residences, parents,
   marginal mentions. For each, decide: proposed, already in the tree, or blocked. Propose
   what you are unsure of too (a doubtful reading, a probable identity), with its honest
   `quality` and your doubt in `remarks`: the user decides when reviewing. Keep a fact only in the
   transcription when it is illegible, and say why in `remarks`. Do not give every fact the act's
   date: an occupation or residence may be current, former or undated.
3. Check that the act is about the person in the tree: call `person_sheet` and compare names,
   dates, places and parents. If it clearly does not match, stop and tell the user why. If it
   probably matches but something is uncertain (a parent's name missing, a date a few years
   off), propose it anyway with a lower `quality` and say what is uncertain in `remarks`. If you
   cannot tell whether it is about this person at all (an age that contradicts the tree, two
   people of the same name in the parish), do not propose it and never write it as a note in the
   tree: it is a clue. Note it with `note_research` `clue` on the person's lead (the event it
   would be, the viewer page, and what speaks `for` and `against`), and tell the user. It waits in GEDRoots' Research page until another record proves or rules it out.
4. Call `file_context` if you have not yet: read the user's description of the file and the
   latest sources and people. Then call `search_sources` with the register or the archive's name.
   Reuse the repository, and write the source the way this file does: reuse a register's source
   when the file keeps one source per register, otherwise propose a new source, with a title,
   register (PUBL) and call number shaped like the file's examples. When the tree has no
   repository for the archive (search it by its name and its town first), describe it in the new
   source's `repository`: its usual full name, postal address and website.
5. Call `search_people` with the `act`: its event, date and everyone named (give the id of the
   principal person already identified). Decide for each person who they are: `link` someone of
   the tree (check a likely candidate with `person_sheet`), `create` a new person, or `mention`
   witnesses and neighbours unless the user wants them in the tree. A parent or witness is often
   already in the tree from another act: never create anyone before resolving who they are, and
   never take an empty search for an absent person (try other spellings with `query`).
6. Call `propose_act` with that `identity` for each person (and `not` for the candidates you
   checked and found to be someone else), and every other fact in that person's `facts`
   (birth date in a baptism, marginal mention, earlier residence…), each with its own date or
   none. Give the act's `quality`, honestly: 3 when the act itself records the event at the
   time, clearly read and about the right person; 2 for secondary evidence (written later or
   second-hand: a table, an index, a death act for the birth); 1 when the reading or the identity
   is doubtful; 0 for a guess. A fact the act supports less than its event (the birth date or
   birthplace in a marriage or death act, an age) gets its own `quality`. GEDRoots writes it on
   the citation (GEDCOM `QUAY`) and shows it to the user. Check the `will_write` list of the answer against your inventory: when it says the tree
   writes a close occupation another way, and it is the same one, propose again with `replaces`
   and the tree's wording. Give the place as
   the tree names it. A place the tree does not have yet (or has without coordinates) is located
   with `locate_place`, one place at a time: give the people of the act already in the tree and
   the act's date, and weigh each candidate's `fit` and `why` against what the act says (the
   département, the parish), not merely the distance. Give the chosen candidate's `place` as is
   (never a place written by hand in another format) and, in `locations`, its coordinates, `osm`
   and the `reason` of your choice. When no candidate clearly fits, propose the place without a
   location, in the tree's place `format`; the answer's `unlocated_places` lists the new places
   still without coordinates.
   Give the transcription and the act's page in the archive's viewer (`url`, its permanent link
   such as an ark when the viewer offers one): GEDRoots keeps it as the source's web link, so the
   user can find the original again. The link goes only in `url`, never in `page` (the act's
   number and image) or in `remarks`. Add the downloaded file (`file`, with `pdf_page` for a PDF)
   when you have one: GEDRoots keeps the image too.
7. If GEDRoots refuses the proposal, read the reason, fix the proposal and propose it again.
   Nothing is written until the user compares and saves it: tell them it is waiting in GEDRoots'
   To Review page, and list the facts you left out and why. Give the same remarks in
   `remarks` of `propose_act`: the user reads them beside the act while reviewing it, and they stay
   with the act in the tree as the note of its citation.
8. The answer gives the proposal's id and the temporary id of each new person (`new_people`). A
   person found in another act before the user saved the first one (a father in two baptisms) is
   linked with that temporary id, never created twice. If you find a mistake in a proposal still
   waiting, propose it again with `replaces` (its id), or withdraw it with `withdraw_proposal`.
   If the proposal is already saved, or the act shows the tree is wrong (a given name misread, a
   name the act spells otherwise, a wrong date, a wrong parent, two people of the tree who are
   one), propose the correction with `propose_change`, with the reason and the evidence: never
   leave it for the user to fix by hand. `will_write` tells you when a person you link is named
   otherwise in the act.
9. When `news` says the user saved one of your proposals, link its new people by the id they now
   have in the tree. Before the next proposal, read the people again (`person_sheet`): the user
   may have changed them in record entry.
10. When the user asks for changes on a proposal (`changes_requested` in `news` or
   `resume_research`), read their `note` as an instruction about that act: read the act again,
   then propose it again with `replaces` and the proposal's id, saying in `remarks` what you
   changed. If you think they are wrong, say why in `remarks` rather than ignoring them.

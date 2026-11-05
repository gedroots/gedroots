Read a downloaded act and propose it to my family tree open in GEDRoots.

{target}

How to work:

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
   marginal mentions. For each, decide: proposed, already in the tree, kept in the transcription
   only (say why in `remarks`: uncertain reading, contradiction…), or blocked. Something
   certain must never stay only in the transcription without a reason. Do not give every fact the act's date: an
   occupation or residence may be current, former or undated.
3. Check that the act is about the person in the tree: call `person_sheet` and compare names,
   dates, places and parents. If it does not match, stop and tell the user why.
4. Call `file_context` if you have not yet: read the user's description of the file and the
   latest sources and people. Then call `search_sources` with the register or the archive's name.
   Reuse the repository, and write the source the way this file does: reuse a register's source
   when the file keeps one source per register, otherwise propose a new source, with a title,
   register (PUBL) and call number shaped like the file's examples. When the tree has no
   repository for the archive (search it by its name and its town first), describe it in the new
   source's `repository`: its usual full name, postal address and website.
5. Call `match_people` with the act's event, date and everyone named (give the id of the
   principal person already identified). Decide for each person who they are: `link` someone of
   the tree (check a likely candidate with `person_sheet`), `create` a new person, or `mention`
   witnesses and neighbours unless the user wants them in the tree. A parent or witness is often
   already in the tree from another act: never create anyone before resolving who they are, and
   never take an empty search for an absent person (try other spellings, `search_people`).
6. Call `propose_act` with that `identity` for each person (and `not` for the candidates you
   checked and found to be someone else), and every other certain fact in that person's `facts`
   (birth date in a baptism, marginal mention, earlier residence…), each with its own date or
   none. Check the `will_write` list of the answer against your inventory: when it says the tree
   writes a close occupation another way, and it is the same one, propose again with `replaces`
   and the tree's wording. Give the place as
   the tree names it. A place the tree does not have yet (or has without coordinates) is located
   with `locate_place`, one place at a time: give the people of the act already in the tree and
   the act's date, then choose the candidate that fits the act's jurisdictions and is close to
   the family's other events, not merely the first; give its `place` and, in `locations`, its
   coordinates and `osm`. When no candidate clearly fits, propose the place without a location.
   Give the transcription, and both the downloaded file (`file`, with `pdf_page`
   for a PDF) and the act's page in the archive's viewer (`url`, its permanent link such as an
   ark when the viewer offers one): GEDRoots keeps the image and a web link, so the user can see
   the act and find the original again.
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
   name the act spells otherwise, a wrong date), propose the correction with `propose_change`
   without `issue` (`set_name`, `add_name` for a variant spelling, `set_event`, `unlink_relative`
   and `link_relative` for a wrong parent, `add_note` when two acts disagree…), with the reason
   and the evidence: never leave it for the user to fix by hand. `will_write` tells you when a
   person you link is named otherwise in the act.
9. Every answer of GEDRoots carries `news` when the user saved, rejected or edited one of your
   proposals since your last call. Tell the user, and once a proposal is saved, link its new
   people by the id they now have in the tree. Before the next proposal, read the people again
   (`person_sheet`): the user may have changed them in record entry.

Never show GEDRoots ids (I12, S4…) to the user.

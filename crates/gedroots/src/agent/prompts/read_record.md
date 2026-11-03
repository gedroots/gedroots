Read a downloaded act and propose it to my family tree open in GEDRoots.

{target}

How to work:

1. Open the file and read the act itself (for a PDF register, find the page of the act). Write a
   faithful transcription: keep the spelling of names and places as written, mark doubtful
   readings with [?] and illegible words with [...]. Never fill in what you cannot read.
2. From the act, note: the event (an event without its own GEDCOM tag, such as military service,
   is `EVEN` with its `event_type`) and its date (a French Republican date stays in that calendar,
   in GEDCOM 7 form such as `FRENCH_R 30 GERM 4`; otherwise `14 MAR 1912`), the place, the
   reference (act number, page or image), and every person with their role in the act (principal,
   father, mother, spouse, witness, godfather, godmother, declarant, officiator…), age,
   occupation, residence and birthplace when written. Inventory every fact the document gives,
   not only its main event: birth, baptism, marriage, death, occupations, residences, parents,
   marginal mentions. For each, decide: proposed, already in the tree, kept in the transcription
   only (say why: uncertain reading, contradiction…), or blocked. Something certain must never
   stay only in the transcription without a reason. Do not give every fact the act's date: an
   occupation or residence may be current, former or undated.
3. Check that the act is about the person in the tree: call `person_sheet` and compare names,
   dates, places and parents. If it does not match, stop and tell the user why.
4. Call `search_sources` with the register or the archive's name. Reuse the repository, and the
   source when the file keeps one source per register (`acts` above 1); otherwise propose a new
   source with the register (PUBL), the repository and the call number.
5. Call `match_people` with the act's event, date and everyone named (give the id of the
   principal person already identified). Decide for each person who they are: `link` someone of
   the tree (check a likely candidate with `person_sheet`), `create` a new person, or `mention`
   witnesses and neighbours unless the user wants them in the tree. A parent or witness is often
   already in the tree from another act: never create anyone before resolving who they are, and
   never take an empty search for an absent person (try other spellings, `search_people`).
6. Call `propose_act` with that `identity` for each person (and `not` for the candidates you
   checked and found to be someone else), and every other certain fact in that person's `facts`
   (birth date in a baptism, marginal mention, earlier residence…), each with its own date or
   none. Check the `will_write` list of the answer against your inventory. Give the place as
   the tree names it, the transcription, and both the downloaded file (`file`, with `pdf_page`
   for a PDF) and the act's page in the archive's viewer (`url`, its permanent link such as an
   ark when the viewer offers one): GEDRoots keeps the image and a web link, so the user can see
   the act and find the original again.
7. If GEDRoots refuses the proposal, read the reason, fix the proposal and propose it again.
   Nothing is written until the user compares and saves it: tell them it is waiting in GEDRoots'
   To Review page, and list the facts you left out and why.
8. Before the next proposal, read the people again (`person_sheet`): the user may have saved,
   changed or rejected the previous one in the meantime.

Never show GEDRoots ids (I12, S4…) to the user.

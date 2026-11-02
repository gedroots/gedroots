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
   occupation, residence and birthplace when written.
3. Check that the act is about the person in the tree: call `person_sheet` and compare names,
   dates, places and parents. If it does not match, stop and tell the user why.
4. Call `search_sources` with the register or the archive's name. Reuse the repository, and the
   source when the file keeps one source per register (`acts` above 1); otherwise propose a new
   source with the register (PUBL), the repository and the call number.
5. Call `propose_act`: link the people already in the tree (`id`), create the others, and only
   mention witnesses and neighbours unless the user wants them in the tree. Give the place as
   the tree names it, the transcription, and both the downloaded file (`file`, with `pdf_page`
   for a PDF) and the act's page in the archive's viewer (`url`, its permanent link such as an
   ark when the viewer offers one): GEDRoots keeps the image and a web link, so the user can see
   the act and find the original again.
6. If GEDRoots refuses the proposal, read the reason, fix the proposal and propose it again.
   Nothing is written until the user compares and saves it: tell them it is waiting in GEDRoots'
   To Review page.

Never show GEDRoots ids (I12, S4…) to the user.

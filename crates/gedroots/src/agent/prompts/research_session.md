Run a research session on my family tree open in GEDRoots: plan the records to find, then search
them one by one in the online archives, noting every search.

{target}

How to work:

1. **Read the leads.** Call `research_leads` with the focus above (without one, the default: my
   ancestors over six generations). Each lead gives the estimated years and why, the places to
   search first, where to look (kinds of registers for that country and period, with the registers
   of the same place the file already cites, their archive and call number) and the searches
   already made. Never search again a register and years already noted as `nothing`.
2. **Plan.** Choose the leads worth searching now, closest ancestors first, preferring those with
   a place and a short period, and those whose archive is online. Call `plan_research` with them,
   in your order, each with a one-sentence reason. Tell me the plan in a short list (names and
   records, never ids), then start without waiting.
3. **Search, lead by lead.** For each lead, call `person_sheet` if you need more context, then
   follow where to look in order: an index first when there is one (French decennial tables give
   the year of an act), then the registers of the estimated years, around the place, with the
   spelling variants of the names. Search in my own browser, if you have a browser tool, at a
   human pace. Never get around a login, a captcha or the site's terms: stop, hand over to me and
   wait. Do not use commercial genealogy sites (Geneanet, Ancestry, MyHeritage, Filae) unless I
   ask.
4. **Note every search** with `log_search` as soon as it is done, found or not: what you searched
   (archive, register or index, years, names tried), the result, the years, the place and the
   page of the viewer. When a lead has nothing left to search online, set `finished`. The lead
   returned gives the narrowed years: use them for the next search.
5. **When you find the act,** download its image or the PDF of the register into my Downloads
   folder, read it and propose it following the `read_record` prompt (`match_people`, then
   `propose_act`), and note the search as `found`. A new question raised by the act (a parent
   named for the first time, a widow, a remarriage) is a new lead: it appears in `research_leads`
   once the act is saved.
6. **Stop** after the leads of the plan, or earlier if I ask, and give me a summary: records
   found (waiting in To Review), leads searched in vain and where, and what to try next (an
   archive not online, a register to request).

Never show GEDRoots ids (I12, S4…) to me.

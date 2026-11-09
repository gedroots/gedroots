# AI agents (MCP)

GEDRoots can serve the [Model Context Protocol](https://modelcontextprotocol.io) so that an AI
agent, such as Claude Code, reads the family tree open in the app and helps with research:
which records are still missing, where and when to look for them. When the agent finds a record
in the archives, it proposes it; the act waits in the app's **To Review** page until a person
compares it with the tree and saves it.

## Connecting

GEDRoots › Connect an AI Agent…, turn on **Allow AI agents**, then pick the agent in the list. The
window gives the shortest way to connect each one (the token is masked on screen, the copy
buttons give the real value):

| Agent | How |
| --- | --- |
| Claude Code, Gemini CLI | A command to run once in a terminal (`claude mcp add --transport http …`, `gemini mcp add --transport http …`) |
| Codex | A block to add to `~/.codex/config.toml` (`url` and `http_headers`) |
| Claude Desktop | A block for `claude_desktop_config.json`, through `npx mcp-remote` (Node.js), since its connectors cannot reach `127.0.0.1` |
| Cursor, VS Code | **Add to Cursor / VS Code** opens the editor's install link (`cursor://anysphere.cursor-deeplink/mcp/install`, `vscode:mcp/install`); the `mcp.json` block is there as a fallback |
| Other MCP client | Address, transport (Streamable HTTP) and `Authorization` header |

The server runs while GEDRoots is open and the setting is on. The window shows the last agent
that connected (the name it gives when it opens its MCP session), and remembers the chosen agent
in the machine's settings. The commands and blocks are built in `crates/gedroots/src/agent/clients.rs`.

## Tools

The tools return structured JSON (`outputSchema`). GEDRoots shows one tree per window: every
tool takes an optional `tree` (a name, file name or id, as `file_context` gives them in `trees`) and otherwise
works on the tree of the window in front. A proposal goes to the To Review page of its own tree.
All are read-only (`readOnlyHint`) except `propose_act`, `propose_change`, `propose_chapter` and
`withdraw_proposal`, which write nothing either: they only add or take back a proposal to review;
and `note_research`, which writes only the research log (see below), never the tree's data.

The file is the agents' memory: what was searched, the clues, where each agent stopped and what
became of every proposal are kept in the `.ged`, so that any agent, in any session, carries on
where another stopped (`resume_research`), with no memory of its own.

| Tool | Returns |
| --- | --- |
| `file_context` | The trees open in GEDRoots (`trees`: name, file, id, number of people, families, sources and media, root person, the one in front first), then how the file is written: the description the user wrote (File Settings › General, `HEAD.NOTE`, word for word), language, place format, events, custom event types and occupations in use with their counts, media folders and naming, and the latest sources, people, families, media and repositories changed as raw GEDCOM (`examples`). Also served as the resource `gedroots://file/context` |
| `search_people` | With `query`: people matching a search, as in the app's search box (advanced keywords, spellings found by sound). With `act`: for the people named in an act (role, names, sex, age, act date), the people of the tree who could be them, with the reason and whether the match is likely |
| `person_sheet` | Events with their position in the record, date, place, street address (`ADDR`), age, cause, cited sources (`PAGE`), media, associates and notes; parents (and how, when not by birth), unions, children, the person's notes and media; Sosa number |
| `resume_research` | Where the research stands, and the tree's language to write the log in: the leads in progress anywhere in the tree (planned, or with a clue or a handoff) with their whole log, the other leads to search for a focus (search syntax, default `sosa:1-63`), each with estimated years and why, places to search first and where to look, the proposals the user sent back asking for changes, the proposals waiting for review and what became of the others (saved, rejected with the user's reason, edited, withdrawn, replaced). With `person`: everything about that person |
| `note_research` | Writes in a lead's log: `plan` (a lead about to be searched, with the reason; state `planned`), `search` (what was searched, whatever its result: `found`, `nothing`, `partial`; years, place, viewer page; `finished` when nothing is left), `clue` (a record that may be the one, with what speaks `for` and `against`: it stays in the Research page, never in the tree) or `handoff` (where the lead stands and the `next` search, when the agent stops) |
| `quality_issues` | The Data Quality page's issues, filtered by category, kind or record and paged (`offset`, `limit`): message, records and places involved, whether the page fixes it in one click, whether it can be hidden, counts per category |
| `search_sources` | Sources (register, repository, call number, number of acts) and repositories, to cite an existing one |
| `locate_place` | For one place of an act: the tree's places of that name, the located places of the events of the act's people and their relatives (closest in time first), and the gazetteer's candidates (OpenStreetMap, through Photon, biased towards those places) in the file's place format, best fit first, each with its `fit`, its reasons (`why`) and its distance to the closest of those events; for the tree's places, whether they follow the format and their completed name |
| `propose_act` | Adds an act to the To Review page: event, date, place, street address, source, reference, quality of the evidence (`QUAY`), transcription (plain text, only the act), people with their role, downloaded file, and the agent's remarks on the act (shown beside it, then kept as the note of its citation). Answers its id, what it will write and the temporary id of each new person; with `replaces`, corrects a waiting proposal |
| `withdraw_proposal` | Takes back a waiting proposal |
| `propose_change` | Adds typed edits to the To Review page, with the reason and the evidence: a correction read in a document, a change for a Data Quality issue (the page's own fix `apply_fix`, or `hide_issue` for a correct one), or a merge of two people or places. The answer says whether it resolves its issue and lists the issues it would create; `replaces` corrects a waiting one |
| `read_book` | The tree's family books, or the chapters of one: each chapter is a generation, with its people (id, name, years, number, how much of a record the book gives them, facts known) and what the agent already wrote |
| `propose_chapter` | Adds what the agent wrote for one chapter of a book to the To Review page: an introduction and people's narratives, in `text` blocks (people cited as `{I12}`, checked to be in the tree), `context` blocks (with their sources) and `hypothesis` blocks (with their basis), and `visualization` blocks (a chart GEDRoots draws from the tree, for the book's people or another person's ancestors or descendants, with an optional comment). A new proposal for the same chapter takes the place of the waiting one. In To Review, context, hypotheses and visualizations can be unticked; only the ticked ones are saved into the book |
| `read_visualization` | Without a name, the visualizations GEDRoots draws (fan chart by origin, places and migrations, lives, ancestors found, longevity, occupations): what each shows and when it suits a book. With one, its data for a person's ancestors or descendants: the Visualizations page's table (one row per person, place, generation or occupation), the legend of its colours and its caption |

Ids (`I12`, `F3`…) are only for passing between tools; the server's instructions ask the agent
not to show them.

How the file is written (one source per act or per register, source titles, name forms) is not
checked: `file_context` shows it to the agent, and the user reviews each proposal. The file's
description is where the user writes their own conventions, in plain words, for agents to follow.

## Prompts

The server also serves its working methods as MCP prompts, so no plugin is needed: in Claude
Code they appear as slash commands once the server is connected.

| Prompt | Arguments | Does |
| --- | --- | --- |
| `research` | `person`, `event`, `focus` (all optional) | The research loop: resumes with `resume_research`, plans a few leads, searches each in the browser at a human pace, notes every search and clue with `note_research`, proposes each act found right away, with the same method as `read_record`, notes a handoff on each lead left open, then sums up what was found, what was searched in vain and what to try next |
| `read_record` | `file`, `person` (both optional) | Transcribes the act, checks it against the person's sheet, identifies each person with `search_people` (`act`), picks or describes the source with `search_sources`, then calls `propose_act` and fixes it if refused |
| `review_quality` | `category`, `person` (both optional) | Goes through the Data Quality issues with `quality_issues`, checks each one against the sheets and sources, then proposes a fix, an edit or hiding it with `propose_change` |
| `write_book` | `book`, `chapter` (both optional) | Reads the book with `read_book`, then chapter by chapter reads the people's sheets, searches the historical context in reliable sources, and proposes the narratives, context and hypotheses with `propose_chapter` |

```text
/mcp__gedroots__research "Jean MARCHAND" birth
/mcp__gedroots__research --focus sosa:1-15
/mcp__gedroots__read_record ~/Downloads/act.pdf
```

Their text lives in `crates/gedroots/src/agent/prompts/`. They tell the agent never to bypass a
login or a captcha, and never to show ids.

## Research leads and log

A lead is computed from the file, never stored (`crates/gedcom/src/leads.rs`), and disappears once
its record is cited. A baptism counts as a birth, a burial as a death.

- **Estimated years**: a death falls after the last time the person was seen alive (their events,
  the events they attended as a witness or godparent, their children's births) and before a
  spouse's remarriage, at most 105 years after the birth; a marriage after both spouses reached the legal
  age of their country and before the first child; a birth 15 years before the first sighting, after a parent's birth.
  Searches that found nothing at an end of the period narrow it.
- **Places**: the last places where the person was seen for a death, the bride's places for a
  marriage, the parents' marriage and siblings' births for a birth, the birth place for parents. A
  locality counts with its municipality (place format).
- **Where to look**: kinds of records by country and period, from the [Atlas](atlas.md)
  (country named in the place, otherwise the file's most frequent country; the generic rules for
  countries without a file and periods a file does not cover). Records still closed to the public
  keep their years and give `public_until`: later ones are given on request to the people
  concerned and their descendants. The legal age at marriage of the country bounds a marriage.
  Civil and church registers and their indexes list the registers of the same town the
  file already cites (sources whose title or publication names the town, “St” read as “Saint”,
  and acts filed in its registers: same call number cited twice or same series; one per call
  number and period:
  register, source, archive, call number, number of events, years), close in time, those of the
  same kind of event first.

What the agent plans, searches, suspects (clues) and leaves for the next one (handoffs) is kept in
the file, under the person (or the family for a marriage), with the agent's name, as a
`_GR_TASK` structure ([extensions](extensions.md#_gr_task); `crates/gedcom/src/research_log.rs`).
It is written at once, not reviewed: it is the trace of the research, not data of the tree. Each
change is one undo step, and the top bar says what the agent noted. A found record is still
proposed with `propose_act` and reviewed.

The same leads and log are on the app's Research page (sidebar): the list of the focus by
state (in progress, to find, no result, ignored; the leads in progress show whatever the focus),
and for each lead the agent working on it now with its last action, its clues with what speaks for
and against, the last handoff (folded) and its next search, what is known with the record to find,
its estimated years and why, where to look and the log, each agent's entries marked with its name. What agents do live comes from their
tool calls and is never kept (a green dot on Research while one is at work, gone two minutes after
its last call); what they note is in the file. Copy Request for the Agent (the lead's context
menu) puts a request for one lead on the clipboard, to paste in the agent's conversation.

## Proposals and review

`propose_act` takes the same model as record entry (Enter Record…, `crates/gedcom/src/act.rs`): an
event (any event without its own tag, such as military service, is `EVEN` with its
`event_type`), the people with their standard GEDCOM 7 role and their `identity`, always given
explicitly: `link` (an existing person, with `id`), `create` or `mention`, and the other facts the act
gives about each person (`facts`: a birth date in a baptism, a death in a marginal mention, an
undated occupation…, each written with the act's citation), the reference (`PAGE`), the transcription and the source.
The agent also gives its confidence as `quality`, the standard GEDCOM 7 quality of the evidence
(`QUAY` 0 to 3, the Quality pop-up of record entry), written on every citation of the act; a fact
the act supports less than its event (a birth date in a death act) carries its own `quality`.
Agents are asked to propose each act as soon as they have read it, even when unsure, with an
honest quality and their doubts in `remarks`, rather than holding back what is not certain. Nothing raw goes
into the file: GEDRoots writes the whole chain itself, repository → source → file → event →
people (`crates/gedcom/src/proposal.rs`).

Before a proposal reaches the review page it must fit the tree. Otherwise it is refused with the
reason, which the agent reads to fix it:

- the source and repository must exist, or the new source must name its register. A new source
  may also create its repository (`source.repository`: name, postal address, website) when the
  tree has none for that archive; a repository of the tree with the same name (case, accents and
  punctuation aside) is cited instead, so two proposals naming the same new archive create it
  once. The review page shows it as created in the attachment;
- a parent other than the one already in the tree is refused (link that person, or mention the
  other one);
- a person to create must not resemble someone of the tree (same names, or a relative of a linked
  person, with compatible dates; `Act::suggestions`, as in record entry): the refusal lists the
  candidates, and the agent links one or lists in `not` those it checked and found to be someone
  else. The review page still shows “Resembles …” under such a person;
- linked people must exist; dates must be valid, and are rewritten in
  GEDCOM 7 syntax; a place named like a place of the tree becomes that place. The place is the
  municipality and its jurisdictions: a street, number or building goes in `address` (the event's
  or a fact's `ADDR`), shown on its own line in the review page, so that one street does not become
  a new place.

A proposal is checked against the tree as if every waiting proposal were saved
(`propose::Hypothesis`). Each new person gets a temporary id (`new-3`), returned in `new_people`:
another proposal links that id to name the same person, and creating them again is refused like
any other likely duplicate. Both proposals then create the person; whichever the user saves
first does, and the other is relinked to the saved person. The review page shows “Also in …”
under such a person. Every tool answer carries `news` when the user decided about proposals since
that session's last call (saved, with the ids the new people now have; rejected, with the reason;
changes requested, with what to change; edited in record entry): agents are not pushed anything between their turns, so the news rides on their next call. A correction (`replaces: "P3"`) takes the place of the waiting proposal, and
the people of the replaced proposal keep their temporary ids when linked again. The research
tools (`person_sheet`, `resume_research`, `note_research`) also take a temporary id once its proposal is saved; while it waits, they answer that the person is
not in the tree yet and which proposal creates them. `search_people` falls back to names that
sound alike (Carl finds Karl) when no name matches as written.

The downloaded file takes the format of the acts already attached to the file's sources: the
act's page of a PDF register becomes a JPEG image (rendered in pure Rust, on every platform) kept
in the app's cache until the act is saved. The act's page in the archive's viewer (`url`) becomes a
web link of the source, like the links added in the app: a media whose `FILE` is the address
(`FORM text/html`), titled after the source and the reference. The image and the web link stay two
separate media: the source shows the image and a link button, and a web link is never offered
as an act's image. The `url` needs no `file`: an act proposed with only its link gets the web link,
written when the act is saved.

The **To Review** page (sidebar) shows, for each proposal,
the act's image, its quality next to the agent's name (a questionable or unreliable proposal is
also flagged in the list), a comparison of what the tree holds and what is proposed (each person can be
unchecked), the **Facts** it gives (each one can be unchecked; the occupations, residences and
estimated births deduced from the act are listed there too, so nothing is written unseen; the
answer of `propose_act` lists the same lines in `will_write`), the attachment (repository, source, file, web link, event), the agent's remarks (saved with the act as the note of its citation, shown under the source in the event's sheet), the transcription line by line and, on demand,
the exact GEDCOM lines that will change, computed on a copy of the file. **Save to Tree** (⌘↩)
writes everything at once, as a single undo step, and stores the image next to the `.ged`,
named by File Settings › Media; **Reject…** (⌘⌫) asks why, in a popover: **Reject** drops it
(with the reason, if any), **Request Changes** (⌘↩) sends it back to its agent with what to
change. The agent reads it in `news` at its next call, or in `resume_research`
(`changes_requested`), and proposes it again with `replaces`; the correction comes back marked
**Corrected**, with **Your Request** above it. Nothing is written in either case; **Edit in Record Entry…** opens it in
the act editor. Every proposal (act, change, chapter) is also kept in the file as a `_GR_PROP`
record ([extensions](extensions.md#_gr_prop); `crates/gedcom/src/agent_proposals.rs`): while it
waits, with the agent's request, queued again under the same id when the file opens (one that no
longer fits the tree is withdrawn, with the reason); once decided, with its outcome. It is not part
of the tree's history: Undo and Redo leave it as it is.

### Locating and completing places

A place the tree does not have yet can come with its coordinates (`locations`: the place,
latitude, longitude, the OpenStreetMap identifier, written as the place's `MAP` and `EXID`, and
the agent's reason for choosing it). The agent first calls `locate_place`, one place at a time
(at most one request every 1.1 s, never in bulk). Homonyms are common, so each candidate comes
scored, best first, with what speaks for and against it (`fit`: likely, possible or unlikely,
and `why`): its distance to the places where the act's people and their relatives lived (the
closest in time weigh most), the jurisdictions the agent named (“Saint-Martin, Manche”), its
level (a département is not a municipality) and its country. The score informs, the agent
decides against what the act says, and the user sees in To Review how far the place is from the
family's events and the agent's reason.

Candidates come in the file's place format (`HEAD.PLAC.FORM`), each jurisdiction recognized by
its name in six languages, or by the tree's values when the name says nothing (postal codes,
countries); an unknown one is left empty. A new place must follow that format, so that it
splits into its jurisdictions like the others: a place written by hand in another one
(“Hérault, Occitanie, France” in a file of six jurisdictions) is refused with the format and an
example. The answer of `propose_act` lists the new places still without coordinates
(`unlocated_places`).

A place of the tree without coordinates, or outside the format (Data Quality › Places), is
located the same way: `locate_place` gives it `completed` (missing postal code, département or
region filled from the best candidate, the tree's spelling kept), and the agent proposes
`set_coordinates` then `rename_place`. In the app, Locate on a place's record completes it the
same way. A location is refused for a place the proposal does not name, or one the tree has
already located (moving it is a `propose_change` with `set_coordinates`).

## Privacy and safety

- **Living people are not set apart.** The agent reads every person the way the app shows them
  and can research and log searches for anyone: nothing it does reaches the tree without your
  review, except its research log.
- **Local only.** The server listens on `127.0.0.1:47280`, requires the token in the
  `Authorization` header and rejects requests sent by a web page (`Origin` header).
- **The agent never writes the tree's data.** It can only propose; a person reviews, compares and
  saves. Only its research log (plan and searches) is written directly, with an undo point.
- The setting, the token and the chosen agent belong to the machine, not to the file: they are kept in
  `GEDRoots/agents` in the platform's configuration folder (`~/Library/Application Support` on
  macOS, `~/.config` on Linux, `%APPDATA%` on Windows), never in the `.ged`.

## Changes

`propose_change` (`crates/gedcom/src/change.rs`) proposes a list of typed `edits` (never raw
GEDCOM), with the `reason` (what the agent read) and the `evidence` (a source of the tree, the
viewer page or a quote). Any edit may be proposed on any record, for a Data Quality issue
(`issue`, the id `quality_issues` gives it) or without one. Without an issue, the item is titled
**Correction** in To Review and the evidence is required. The edits:

- **names and events**: `set_name` (given name and/or surname of one of the person's names, the
  rest of the name kept), `add_name` (another `NAME`, with its `TYPE`, for a variant spelling),
  `set_event` (value such as an occupation, date, place, street address and/or `cause`),
  `add_event` (`OCCU` with its value, `RESI` with its address…), `delete_event`, `set_sex`,
  `set_age` (`AGE`; on a family event, the age of one of the couple), `add_associate` and
  `remove_associate` (`ASSO` with its GEDCOM 7 `ROLE` and `PHRASE`);
- **sources and places**: `cite_source`, `set_repository` (an existing repository, or one to
  create, as in `propose_act`), `rename_place` (everywhere it is used, its coordinates kept),
  `set_coordinates` (with the `osm` of the `locate_place` candidate, kept as the place's `EXID`);
- **family links**: `link_relative`, `unlink_relative` (a child taken from a wrong father stays
  with the mother, in a family of their own, and the couple's union stays), `set_pedigree`
  (`FAMC.PEDI`);
- **notes**: `add_note` and `set_note` (empty text removes it), on a record or one of its events;
- **media**: `attach_media` (an existing media, or the file the agent downloaded, which goes
  through the same import as an act's image: copied to the app's cache, a PDF page extracted,
  shown beside the change in To Review, then named and filed according to File Settings › Media
  when the user saves it) and `detach_media` (the media stays in the file);
- **structures an issue designates** (need that `issue`): `set_value`, `delete_structure`,
  `set_media_file`.

Four edits go on their own: `apply_fix` (the Quality page's own fix of an issue with
`automatic_fix`; fixes of the same kind are grouped into one To Review item, applied together),
`hide_issue` (the issue is right: the reason is kept as a `NOTE` under `_GR_IGNORE`, and a
possible duplicate is recorded with `_GR_DISTINCT`), `merge_people` and `merge_places` (two
records become one: those of a possible duplicate, or `keep` and `other`; `choices` gives whose
value to keep for each fact or level that differs). **Compare and Merge…** opens the merge window
with the agent's choices already ticked: the user checks them and merges there.

Each issue lists the edits that usually settle it (`edits`), as a hint only. GEDRoots applies the
change to a copy first and answers whether it `resolves` its issue and which issues it would
create; it refuses a change only when it cannot apply (an unknown record or source, coordinates out
of range, a merge the engine rules out or without a choice for every conflict, no reason, a
correction without evidence). A waiting change is corrected with `replaces` (its id) or withdrawn
with `withdraw_proposal`. When `propose_act` links a person the act names otherwise, its
`will_write` says so and points to this correction.

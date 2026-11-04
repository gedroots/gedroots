# AI agents (MCP)

GEDRoots can serve the [Model Context Protocol](https://modelcontextprotocol.io) so that an AI
agent, such as Claude Code, reads the family tree open in the app and helps with research:
which records are still missing, where and when to look for them. When the agent finds a record
in the archives, it proposes it; the act waits in the app's **To Review** page until a person
compares it with the tree and saves it.

## Connecting

File › Connect an AI Agent…, turn on **Allow AI agents**, then pick the agent in the list. The
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

The tools return structured JSON (`outputSchema`) and work on the file of the most recently
active window. All are read-only (`readOnlyHint`) except `propose_act`, `propose_change` and
`withdraw_proposal`, which write nothing either: they only add or take back a proposal to review;
and `plan_research` and `log_search`, which write only the research log (see below), never the
tree's data.

| Tool | Returns |
| --- | --- |
| `file_info` | File name, counts, root person |
| `file_context` | How the file is written: the description the user wrote (File Settings › General, `HEAD.NOTE`, word for word), language, place format, events, custom event types and occupations in use with their counts, media folders and naming, and the latest sources, people, families, media and repositories changed as raw GEDCOM (`examples`). Also served as the resource `gedroots://file/context` |
| `search_people` | People matching a search, as in the app's search box |
| `person_sheet` | Events with their position in the record, date, place, cited sources (`PAGE`) and media; parents, unions, children; Sosa number |
| `research_leads` | Records still to find for the people of a focus (search syntax, default `sosa:1-63`) or one person: births, marriages and deaths missing or without a source, parents of ancestors at the end of a line. Each lead has estimated years and why, places to search first, where to look, its state and the searches already noted |
| `research_log` | Everything noted for a person and their unions: plan, searches, records found, ignored leads |
| `plan_research` | Saves the agent's plan: the leads it will search, in order, each with a reason (state `planned`) |
| `log_search` | Notes one search, whatever its result (`found`, `nothing`, `partial`): what was searched, years, place, viewer page; `finished` when nothing is left to search |
| `quality_issues` | The Data Quality page's issues, filtered by category, kind or record and paged (`offset`, `limit`): message, records and places involved, whether the page fixes it in one click, whether it can be hidden, counts per category |
| `search_sources` | Sources (register, repository, call number, number of acts) and repositories, to cite an existing one |
| `match_people` | For the people named in an act (role, names, sex, age, act date), the people of the tree who could be them, with the reason and whether the match is likely |
| `propose_act` | Adds an act to the To Review page: event, date, place, source, reference, transcription, people with their role, downloaded file. Answers its id, what it will write and the temporary id of each new person; with `replaces`, corrects a waiting proposal |
| `list_proposals` | Proposals waiting for review (id, `kind`, people with their ids, what each will write), and what became of the others: saved, rejected, edited in record entry, withdrawn, replaced |
| `withdraw_proposal` | Takes back a waiting proposal |
| `propose_change` | Adds a change for one Data Quality issue to the To Review page: the page's own fix (`fix`), edits among those the issue allows (`edit`), merging a possible duplicate (`merge`), or hiding a correct issue (`hide`), with the reason and the evidence. The answer lists the issues the change would create |

Ids (`I12`, `F3`…) are only for passing between tools; the server's instructions ask the agent
not to show them.

How the file is written (one source per act or per register, source titles, name forms) is not
checked: `file_context` shows it to the agent, and the user reviews each proposal. The file's
description is where the user writes their own conventions, in plain words, for agents to follow.

## Prompts

The server also serves three working methods as MCP prompts, so no plugin is needed: in Claude
Code they appear as slash commands once the server is connected.

| Prompt | Arguments | Does |
| --- | --- | --- |
| `research_session` | `focus`, `count` (both optional) | The research loop: reads the leads, saves a plan with `plan_research`, searches each lead in the browser, notes every search with `log_search`, proposes the acts found (`read_record`), then sums up what was found, what was searched in vain and what to try next |
| `find_record` | `person`, `event` (both optional) | Picks a lead with `research_leads`, finds the right archive and register, searches it in the browser at a human pace, downloads the act and goes on with `read_record` |
| `read_record` | `file`, `person` (both optional) | Transcribes the act, checks it against the person's sheet, identifies each person with `match_people`, picks or describes the source with `search_sources`, then calls `propose_act` and fixes it if refused |
| `review_quality` | `category`, `person` (both optional) | Goes through the Data Quality issues with `quality_issues`, checks each one against the sheets and sources, then proposes a fix, an edit or hiding it with `propose_change` |

```text
/mcp__gedroots__research_session sosa:1-15
/mcp__gedroots__find_record "Jean MARCHAND" birth
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

What the agent plans and searches is kept in the file, under the person (or the family for a
marriage), as a `_GR_TASK` structure ([extensions](extensions.md#_gr_task); `crates/gedcom/src/research_log.rs`).
It is written at once, not reviewed: it is the trace of the research, not data of the tree. Each
change is one undo step, and the top bar says what the agent noted. A found record is still
proposed with `propose_act` and reviewed.

The same leads and log are on the app's Research page (Go › Research): the list of the focus by
state (to find, no result, ignored), and for each lead its period and why, what is known, where to
look and the log, the agent's searches marked “AI agent”. Person › Research › Copy Request for the
Agent puts a request for one lead on the clipboard, to paste in the agent's conversation.

## Proposals and review

`propose_act` takes the same model as Source › Enter Record… (`crates/gedcom/src/act.rs`): an
event (any event without its own tag, such as military service, is `EVEN` with its
`event_type`), the people with their standard GEDCOM 7 role and their `identity`, always given
explicitly: `link` (an existing person, with `id`), `create` or `mention`, and the other facts the act
gives about each person (`facts`: a birth date in a baptism, a death in a marginal mention, an
undated occupation…, each written with the act's citation), the reference (`PAGE`), the transcription and the source. Nothing raw goes
into the file: GEDRoots writes the whole chain itself, repository → source → file → event →
people (`crates/gedcom/src/proposal.rs`).

Before a proposal reaches the review page it must fit the tree. Otherwise it is refused with the
reason, which the agent reads to fix it:

- the source and repository must exist, or the new source must name its register;
- a parent other than the one already in the tree is refused (link that person, or mention the
  other one);
- a person to create must not resemble someone of the tree (same names, or a relative of a linked
  person, with compatible dates; `Act::suggestions`, as in record entry): the refusal lists the
  candidates, and the agent links one or lists in `not` those it checked and found to be someone
  else. The review page still shows “Resembles …” under such a person;
- linked people must exist; dates must be valid, and are rewritten in
  GEDCOM 7 syntax; a place named like a place of the tree becomes that place.

A proposal is checked against the tree as if every waiting proposal were saved
(`propose::Hypothesis`). Each new person gets a temporary id (`new-3`), returned in `new_people`:
another proposal links that id to name the same person, and creating them again is refused like
any other likely duplicate. Both proposals then create the person; whichever the user saves
first does, and the other is relinked to the saved person. The review page shows “Also in …”
under such a person. Every tool answer carries `news` when the user decided about proposals since
that session's last call (saved, with the ids the new people now have; rejected; edited in record
entry): agents are not pushed anything between their turns, so the news rides on their next call. A correction (`replaces: "P3"`) takes the place of the waiting proposal, and
the people of the replaced proposal keep their temporary ids when linked again.

The downloaded file takes the format of the acts already attached to the file's sources: the
act's page of a PDF register becomes a JPEG image (rendered in pure Rust, on every platform) kept
in the app's cache until the act is saved. The act's page in the archive's viewer (`url`) becomes a
web link of the source, like the links added in the app: a media whose `FILE` is the address
(`FORM text/html`), titled after the source and the reference. The image and the web link stay two
separate media: the source shows the image and a link button, and a web link is never offered
as an act's image. A proposal with a `url` must also give the `file`.

The **To Review** page (sidebar, or Source › Review Agent Proposals) shows, for each proposal,
the act's image, a comparison of what the tree holds and what is proposed (each person can be
unchecked), the **Facts** it gives (each one can be unchecked; the occupations, residences and
estimated births deduced from the act are listed there too, so nothing is written unseen; the
answer of `propose_act` lists the same lines in `will_write`), the attachment (repository, source, file, web link, event), the transcription and, on demand,
the exact GEDCOM lines that will change, computed on a copy of the file. **Save to Tree** (⌘↩)
writes everything at once, as a single undo step, and stores the image next to the `.ged`,
named by File Settings › Media; **Reject** (⌘⌫) drops it; **Edit in Record Entry…** opens it in
the act editor. Proposals live in memory: they are lost when the app closes.

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

## Quality changes

`propose_change` (`crates/gedcom/src/change.rs`) targets one issue by the id `quality_issues`
gives it. Each issue lists the edits it accepts (`edits`): `set_event`, `delete_event`,
`set_sex`, `set_coordinates`, `set_media_file`, `cite_source`, `set_value`,
`delete_structure`. A change is refused, with the reason, when:

- the issue no longer exists (the agent calls `quality_issues` again);
- the edit is not one the issue accepts, or touches a record the issue does not involve;
- `fix` is asked for an issue without an automatic fix, or `hide` without a reason;
- a cited source does not exist, or coordinates are out of range;
- after applying it on a copy, GEDRoots still reports the issue.

Fixes of the same kind are grouped into one To Review item, applied together. The item shows the
values in the tree and the proposed ones, the reason, the evidence and the GEDCOM diff; saving it
is one undo step. Hiding keeps the reason as a `NOTE` under `_GR_IGNORE`, and a possible duplicate
is hidden with `_GR_DISTINCT`. Filiations (parents, unions) cannot be changed this way.

A possible duplicate can also be merged (`merge`). Its issue lists, under `merge`, the person
kept, the person merged into it and the facts that contradict each other (`conflicts`: `NAME`,
`BIRT`…). The agent gives, in `choices`, whose value to keep for each of them; a merge without a
choice for every conflict, for two people the engine rules out (close relatives, lives too far
apart), or without a reason, is refused. In To Review, **Compare and Merge…** opens the merge
window with the agent's choices already ticked: the user checks them and merges there.

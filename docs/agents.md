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
active window. All are read-only (`readOnlyHint`) except three: `propose_act` writes nothing
either, it only adds a proposal to review; `plan_research` and `log_search` write only the
research log (see below), never the tree's data.

| Tool | Returns |
| --- | --- |
| `file_info` | File name, counts, root person |
| `search_people` | People matching a search, as in the app's search box |
| `person_sheet` | Events with date, place, cited sources (`PAGE`) and media; parents, unions, children; Sosa number |
| `research_leads` | Records still to find for the people of a focus (search syntax, default `sosa:1-63`) or one person: births, marriages and deaths missing or without a source, parents of ancestors at the end of a line. Each lead has estimated years and why, places to search first, where to look, its state and the searches already noted |
| `research_log` | Everything noted for a person and their unions: plan, searches, records found, ignored leads |
| `plan_research` | Saves the agent's plan: the leads it will search, in order, each with a reason (state `planned`) |
| `log_search` | Notes one search, whatever its result (`found`, `nothing`, `partial`): what was searched, years, place, viewer page; `finished` when nothing is left to search |
| `search_sources` | Sources (register, repository, call number, number of acts) and repositories, to cite an existing one |
| `match_people` | For the people named in an act (role, names, sex, age, act date), the people of the tree who could be them, with the reason and whether the match is likely |
| `propose_act` | Adds an act to the To Review page: event, date, place, source, reference, transcription, people with their role, downloaded file |

Ids (`I12`, `F3`…) are only for passing between tools; the server's instructions ask the agent
not to show them.

## Prompts

The server also serves three working methods as MCP prompts, so no plugin is needed: in Claude
Code they appear as slash commands once the server is connected.

| Prompt | Arguments | Does |
| --- | --- | --- |
| `research_session` | `focus`, `count` (both optional) | The research loop: reads the leads, saves a plan with `plan_research`, searches each lead in the browser, notes every search with `log_search`, proposes the acts found (`read_record`), then sums up what was found, what was searched in vain and what to try next |
| `find_record` | `person`, `event` (both optional) | Picks a lead with `research_leads`, finds the right archive and register, searches it in the browser at a human pace, downloads the act and goes on with `read_record` |
| `read_record` | `file`, `person` (both optional) | Transcribes the act, checks it against the person's sheet, identifies each person with `match_people`, picks or describes the source with `search_sources`, then calls `propose_act` and fixes it if refused |

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
  spouse's remarriage, at most 105 years after the birth; a marriage after both spouses were 15 and
  before the first child; a birth 15 years before the first sighting, after a parent's birth.
  Searches that found nothing at an end of the period narrow it.
- **Places**: the last places where the person was seen for a death, the bride's places for a
  marriage, the parents' marriage and siblings' births for a birth.
- **Where to look**: kinds of records by country and period. France (country named in the place,
  otherwise the file's most frequent country): decennial tables (1793–1902) first, civil registers
  from 1792, parish registers before, census lists (1836–1936) and probate tables for a death,
  marriage contracts, military registers (men born 1847–1921) for parents. Elsewhere: civil
  registration, church registers, census, probate. Each kind lists the registers of the same place
  the file already cites (register, source, archive, call number, number of events, years), those
  of the same kind of event first.

What the agent plans and searches is kept in the file, under the person (or the family for a
marriage), as a `_GR_TASK` structure ([extensions](extensions.md#_gr_task); `crates/gedcom/src/research_log.rs`).
It is written at once, not reviewed: it is the trace of the research, not data of the tree. Each
change is one undo step, and the top bar says what the agent noted. A found record is still
proposed with `propose_act` and reviewed.

## Proposals and review

`propose_act` takes the same model as Source › Enter Record… (`crates/gedcom/src/act.rs`): an
event (any event without its own tag, such as military service, is `EVEN` with its
`event_type`), the people with their standard GEDCOM 7 role and their `identity`, always given
explicitly: `link` (an existing person, with `id`), `create` or `mention`, the reference (`PAGE`), the transcription and the source. Nothing raw goes
into the file: GEDRoots writes the whole chain itself, repository → source → file → event →
people (`crates/gedcom/src/proposal.rs`).

Before a proposal reaches the review page it must fit the tree. Otherwise it is refused with the
reason, which the agent reads to fix it:

- the source and repository must exist, or the new source must name its register;
- the source follows the file's habit for that register: one source per act, or one per register
  with the act's reference as `PAGE`;
- a parent other than the one already in the tree is refused (link that person, or mention the
  other one);
- a person to create must not resemble someone of the tree (same names, or a relative of a linked
  person, with compatible dates; `Act::suggestions`, as in record entry): the refusal lists the
  candidates, and the agent links one or lists in `not` those it checked and found to be someone
  else. The review page still shows “Resembles …” under such a person;
- linked people must exist; dates must be valid, and are rewritten in
  GEDCOM 7 syntax; a place named like a place of the tree becomes that place.

The downloaded file takes the format of the acts already attached to the file's sources: the
act's page of a PDF register becomes a JPEG image (rendered in pure Rust, on every platform) kept
in the app's cache until the act is saved. The act's page in the archive's viewer (`url`) becomes a
web link of the source, like the links added in the app: a media whose `FILE` is the address
(`FORM text/html`), titled after the source and the reference. The image and the web link stay two
separate media: the source shows the image and a link button, and a web link is never offered
as an act's image. A proposal with a `url` must also give the `file`.

The **To Review** page (sidebar, or Source › Review Agent Proposals) shows, for each proposal,
the act's image, a comparison of what the tree holds and what is proposed (each person can be
unchecked), the attachment (repository, source, file, web link, event), the transcription and, on demand,
the exact GEDCOM lines that will change, computed on a copy of the file. **Save to Tree** (⌘↩)
writes everything at once, as a single undo step, and stores the image next to the `.ged`,
named by File Settings › Media; **Reject** (⌘⌫) drops it; **Edit in Record Entry…** opens it in
the act editor. Proposals live in memory: they are lost when the app closes.

## Privacy and safety

- **Living people are reduced to what identifies them.** A person with no death, burial or
  cremation is treated as living unless their birth, or a date that places it (their own events,
  a union, a child's birth, a parent's birth), is more than 100 years ago. The agent can find and
  link a living person, but only gets their name, their birth (date and place, `born`) and their
  close relatives: their sheet leaves out their other events and notes, and they never get a
  research lead (`crates/gedcom/src/research.rs`).
- **Local only.** The server listens on `127.0.0.1:47280`, requires the token in the
  `Authorization` header and rejects requests sent by a web page (`Origin` header).
- **The agent never writes the tree's data.** It can only propose; a person reviews, compares and
  saves. Only its research log (plan and searches) is written directly, with an undo point.
- The setting, the token and the chosen agent belong to the machine, not to the file: they are kept in
  `GEDRoots/agents` in the platform's configuration folder (`~/Library/Application Support` on
  macOS, `~/.config` on Linux, `%APPDATA%` on Windows), never in the `.ged`.

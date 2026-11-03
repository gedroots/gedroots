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
`withdraw_proposal`, which write nothing either: they only add or take back a proposal to review.

| Tool | Returns |
| --- | --- |
| `file_info` | File name, counts, root person |
| `search_people` | People matching a search, as in the app's search box |
| `person_sheet` | Events with their position in the record, date, place, cited sources (`PAGE`) and media; parents, unions, children; Sosa number |
| `research_gaps` | Births, marriages and deaths missing or without a source among the root person's direct ancestors, by Sosa number, with each person's known places |
| `quality_issues` | The Data Quality page's issues, filtered by category, kind or record and paged (`offset`, `limit`): message, records and places involved, whether the page fixes it in one click, whether it can be hidden, counts per category |
| `search_sources` | Sources (register, repository, call number, number of acts) and repositories, to cite an existing one |
| `match_people` | For the people named in an act (role, names, sex, age, act date), the people of the tree who could be them, with the reason and whether the match is likely |
| `propose_act` | Adds an act to the To Review page: event, date, place, source, reference, transcription, people with their role, downloaded file. Answers its id, what it will write and the temporary id of each new person; with `replaces`, corrects a waiting proposal |
| `list_proposals` | Proposals waiting for review (id, `kind`, people with their ids, what each will write), and what became of the others: saved, rejected, edited in record entry, withdrawn, replaced |
| `withdraw_proposal` | Takes back a waiting proposal |
| `propose_change` | Adds a change for one Data Quality issue to the To Review page: the page's own fix (`fix`), edits among those the issue allows (`edit`), or hiding a correct issue (`hide`), with the reason and the evidence. The answer lists the issues the change would create |

Ids (`I12`, `F3`…) are only for passing between tools; the server's instructions ask the agent
not to show them.

## Prompts

The server also serves three working methods as MCP prompts, so no plugin is needed: in Claude
Code they appear as slash commands once the server is connected.

| Prompt | Arguments | Does |
| --- | --- | --- |
| `find_record` | `person`, `event` (both optional) | Picks a gap with `research_gaps`, finds the right archive and register, searches it in the browser at a human pace, downloads the act and goes on with `read_record` |
| `read_record` | `file`, `person` (both optional) | Transcribes the act, checks it against the person's sheet, identifies each person with `match_people`, picks or describes the source with `search_sources`, then calls `propose_act` and fixes it if refused |
| `review_quality` | `category`, `person` (both optional) | Goes through the Data Quality issues with `quality_issues`, checks each one against the sheets and sources, then proposes a fix, an edit or hiding it with `propose_change` |

```text
/mcp__gedroots__find_record "Jean MARCHAND" birth
/mcp__gedroots__read_record ~/Downloads/act.pdf
```

Their text lives in `crates/gedroots/src/agent/prompts/`. They tell the agent never to bypass a
login or a captcha, and never to show ids.

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

- **Living people are reduced to what identifies them.** A person with no death, burial or
  cremation is treated as living unless they were born more than 100 years ago, as their birth
  shows or the tree places it (`Gedcom::latest_birth_year`): the latest year their own dated
  events and the dated acts cited about them allow (an occupation 15 years earlier), their unions and children (15 years
  earlier), grandchildren and further down, their parents (born at most 60 years earlier, or
  dead), spouses (30 years apart), and the dated events where they are a witness or godparent
  (`ASSO`, 15 years earlier). An open date (“after 1850”) places nobody; an approximate one is taken 10 years later. The agent can find and
  link a living person, but only gets their name, their birth (date and place, `born`) and their
  close relatives: their sheet leaves out their other events and notes, and `research_gaps` skips
  them (`crates/gedcom/src/research.rs`). `quality_issues` lists their issues too, their records
  marked `living`.
- **Local only.** The server listens on `127.0.0.1:47280`, requires the token in the
  `Authorization` header and rejects requests sent by a web page (`Origin` header).
- **Nothing is written by the agent.** It can only propose; a person reviews, compares and saves.
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

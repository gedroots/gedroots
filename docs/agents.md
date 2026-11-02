# AI agents (MCP)

GEDRoots can serve the [Model Context Protocol](https://modelcontextprotocol.io) so that an AI
agent, such as Claude Code, reads the family tree open in the app and helps with research:
which records are still missing, where and when to look for them. When the agent finds a record
in the archives, it proposes it; the act waits in the app's **To Review** page until a person
compares it with the tree and saves it.

## Connecting

1. File › Connect an AI Agent…, turn on **Allow AI agents**.
2. Copy the command and run it once in a terminal:

   ```sh
   claude mcp add --transport http gedroots http://127.0.0.1:47280/mcp \
     --header "Authorization: Bearer <token>"
   ```

The server runs while GEDRoots is open and the setting is on. Any MCP client that speaks
Streamable HTTP with a header works the same way.

## Tools

The tools return structured JSON (`outputSchema`) and work on the file of the most recently
active window. All are read-only (`readOnlyHint`) except `propose_act`, which writes nothing
either: it only adds a proposal to review.

| Tool | Returns |
| --- | --- |
| `file_info` | File name, counts, root person |
| `search_people` | People matching a search, as in the app's search box |
| `person_sheet` | Events with date, place, cited sources (`PAGE`) and media; parents, unions, children; Sosa number |
| `research_gaps` | Births, marriages and deaths missing or without a source among the root person's direct ancestors, by Sosa number, with each person's known places |
| `search_sources` | Sources (register, repository, call number, number of acts) and repositories, to cite an existing one |
| `propose_act` | Adds an act to the To Review page: event, date, place, source, reference, transcription, people with their role, downloaded file |

Ids (`I12`, `F3`…) are only for passing between tools; the server's instructions ask the agent
not to show them.

## Prompts

The server also serves two working methods as MCP prompts, so no plugin is needed: in Claude
Code they appear as slash commands once the server is connected.

| Prompt | Arguments | Does |
| --- | --- | --- |
| `find_record` | `person`, `event` (both optional) | Picks a gap with `research_gaps`, finds the right archive and register, searches it in the browser at a human pace, downloads the act and goes on with `read_record` |
| `read_record` | `file`, `person` (both optional) | Transcribes the act, checks it against the person's sheet, picks or describes the source with `search_sources`, then calls `propose_act` and fixes it if refused |

```text
/mcp__gedroots__find_record "Jean MARCHAND" birth
/mcp__gedroots__read_record ~/Downloads/act.pdf
```

Their text lives in `crates/gedroots/src/agent/prompts/`. They tell the agent never to bypass a
login or a captcha, and never to show ids.

## Proposals and review

`propose_act` takes the same model as Source › Enter Record… (`crates/gedcom/src/act.rs`): an
event, the people with their standard GEDCOM 7 role (each linked to an existing person, created,
or only mentioned), the reference (`PAGE`), the transcription and the source. Nothing raw goes
into the file: GEDRoots writes the whole chain itself, repository → source → file → event →
people (`crates/gedcom/src/proposal.rs`).

Before a proposal reaches the review page it must fit the tree. Otherwise it is refused with the
reason, which the agent reads to fix it:

- the source and repository must exist, or the new source must name its register;
- the source follows the file's habit for that register: one source per act, or one per register
  with the act's reference as `PAGE`;
- a parent other than the one already in the tree is refused (link that person, or mention the
  other one);
- linked people must exist and not be living; dates must be valid, and are rewritten in
  GEDCOM 7 syntax; a place named like a place of the tree becomes that place.

The downloaded file takes the format of the acts already attached to the file's sources: the
act's page of a PDF register becomes a JPEG image (rendered in pure Rust, on every platform) kept
in the app's cache until the act is saved. The act's page in the archive's viewer (`url`) becomes a
web link of the source, like the links added in the app: a media whose `FILE` is the address
(`FORM text/html`), titled after the source and the reference.

The **To Review** page (sidebar, or Source › Review Agent Proposals) shows, for each proposal,
the act's image, a comparison of what the tree holds and what is proposed (each person can be
unchecked), the attachment (repository, source, file, event), the transcription and, on demand,
the exact GEDCOM lines that will change, computed on a copy of the file. **Save to Tree** (⌘↩)
writes everything at once, as a single undo step, and stores the image next to the `.ged`,
named by File Settings › Media; **Reject** (⌘⌫) drops it; **Edit in Record Entry…** opens it in
the act editor. Proposals live in memory: they are lost when the app closes.

## Privacy and safety

- **Living people are hidden.** A person with no death, burial or cremation is treated as living
  unless their birth, or a date that places it (their own events, a union, a child's birth, a
  parent's birth), is more than 100 years ago. A living person's sheet is refused and their name
  is replaced by “Living person” wherever they appear (`crates/gedcom/src/research.rs`).
- **Local only.** The server listens on `127.0.0.1:47280`, requires the token in the
  `Authorization` header and rejects requests sent by a web page (`Origin` header).
- **Nothing is written by the agent.** It can only propose; a person reviews, compares and saves.
- The setting and the token belong to the machine, not to the file: they are kept in
  `GEDRoots/agents` in the platform's configuration folder (`~/Library/Application Support` on
  macOS, `~/.config` on Linux, `%APPDATA%` on Windows), never in the `.ged`.

# AI agents (MCP)

GEDRoots can serve the [Model Context Protocol](https://modelcontextprotocol.io) so that an AI
agent, such as Claude Code, reads the family tree open in the app and helps with research:
which records are still missing, where and when to look for them.

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

All tools are read-only (`readOnlyHint`) and return structured JSON (`outputSchema`). They read
the file of the most recently active window.

| Tool | Returns |
| --- | --- |
| `file_info` | File name, counts, root person |
| `search_people` | People matching a search, as in the app's search box |
| `person_sheet` | Events with date, place, cited sources (`PAGE`) and media; parents, unions, children; Sosa number |
| `research_gaps` | Births, marriages and deaths missing or without a source among the root person's direct ancestors, by Sosa number, with each person's known places |

Ids (`I12`, `F3`…) are only for passing between tools; the server's instructions ask the agent
not to show them.

## Privacy and safety

- **Living people are hidden.** A person with no death, burial or cremation is treated as living
  unless their birth, or a date that places it (their own events, a union, a child's birth, a
  parent's birth), is more than 100 years ago. A living person's sheet is refused and their name
  is replaced by « Living person » wherever they appear (`crates/gedcom/src/research.rs`).
- **Local only.** The server listens on `127.0.0.1:47280`, requires the token in the
  `Authorization` header and rejects requests sent by a web page (`Origin` header).
- **Nothing is written.** Writing (attaching a record found in the archives, creating a source)
  will come as proposals that a person reviews and compares in the app before anything is saved.
- The setting and the token belong to the machine, not to the file: they are kept in
  `GEDRoots/agents` in the platform's configuration folder (`~/Library/Application Support` on
  macOS, `~/.config` on Linux, `%APPDATA%` on Windows), never in the `.ged`.

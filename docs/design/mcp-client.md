# The assistant using MCP servers (0.1.42, finished in 0.1.43)

## Context

Asked for on 2026-11-16: Flow's assistant should connect to an app's MCP
server and control the app through it. This is the other way round from
`flow-mcp` (Flow as a server for outside apps, built and tried live, not
yet committed): here Flow is an MCP **client**, and the assistant drives
apps that have MCP servers.

What was decided:
- **Local and remote servers:** a command Flow starts (stdio), and a URL
  (Streamable HTTP) with a token in a header where it needs one. OAuth
  sign-in comes later.
- **Added in Settings and by a JSON file:** Settings has Add server…, and
  Flow reads `mcp-servers.json` beside config.toml, in the `mcpServers`
  form Claude Desktop and Cursor use, so a config can be pasted in.
- **Every tool listed, each off until turned on:** as skills are, with All
  on and All off per server.
- **Approval chosen per server:** Ask each time (the default), Read-only
  without asking (tools the server marks read-only run, the rest ask), or
  Without asking. Guide mode always asks.

## How it works

**1. Servers** (`core/src/mcp/client/servers.rs`)
- `mcp-servers.json`: `{"mcpServers": {"<name>": {"command", "args",
  "env"} | {"url", "headers", "type"}}}`, read as Claude Desktop, Claude
  Code and Cursor write it (a `type` of `http` or `streamable-http`, or a
  `url` alone, is remote; `sse`, the old transport, is refused by name).
  `${env:NAME}` in `env` and `headers` takes Flow's environment. Flow writes
  the file only when Settings adds, changes or removes a server.
- Flow's own choices go in config.toml under `ai`, so neither programs nor
  the assistant's `flow_settings` can change them:
  - `ai.mcp_servers_on`: the servers to connect to, none at first. A
    server in the file never runs until it's switched on in Settings, so a
    file written by anything else starts nothing.
  - `ai.mcp_tools_on`: each tool turned on, as `<server>:<tool>`.
  - `ai.mcp_approval`: each server's choice (`ask`, `read_only`,
    `allowed`), `ask` where absent.
- A token for a remote server: Settings' **Token…** asks for it in a
  masked box (the key dialog, made to take a server's token too) and keeps
  it in the Keychain or Credential Manager (`secrets.rs`, made to store
  named secrets, `mcp-<server>`), sent as `Authorization: Bearer`.
- `write_file` asks first for `mcp-servers.json` (`is_sensitive_path`).

**2. Connecting** (`core/src/mcp/client/`)
- Each switched-on server has a thread of its own (`flow-mcp-client`),
  which Flow talks to over a channel; answers come back as
  `UserEvent::McpServer`.
- **stdio:** the command started with Flow's environment, the server's
  `env` over it and Flow's apps' `bin/` on the PATH; JSON-RPC a line each
  way, a reader thread matching answers to requests by id; its stderr's last
  lines kept for the error Settings shows; `ping` answered, other requests
  from the server refused; ended with its thread.
- **Streamable HTTP:** each message POSTed with `Accept: application/json,
  text/event-stream`, the headers and token, `Mcp-Session-Id` once
  `initialize` gives one and `MCP-Protocol-Version` after; a JSON answer,
  or an SSE stream read by `provider::read_sse` until the answer with its
  id; through `provider::http_agent`.
- Both: `initialize` (Flow's versions, newest first; `clientInfo` flow),
  `notifications/initialized`, `tools/list` (every page), and on
  `notifications/tools/list_changed` again. A call is `tools/call`; its
  `content` comes back as text (text parts; an image or resource said in a
  line, a resource's text where it has one), `isError` as the tool's error.
- A server that fails says why in Settings and is tried again when it's
  switched on again or Flow starts.

**3. The tools offered** (`core/src/ai/tools.rs`, the providers)
- `AiSettings::server_tools`: the tools turned on, of servers connected:
  each one's name for the model, `mcp__<server>__<tool>` (letters, digits,
  `_` and `-`, at most 64, shortened with a hash where longer), its
  description (`[<server>] …`), its schema and whether it's read-only.
  `tools::offered(&settings)` is `functions(toolset)` with them after, and
  each provider's `body()` takes that.
- Gemini takes only part of JSON Schema, so its client cleans every tool's
  schema (`$schema`, `$ref`/`$defs`, `additionalProperties`, `const`,
  `type` lists).
- `parse_call` turns a name starting `mcp__` into `ToolCall::Server { name,
  args }`; the App finds its server and tool, and one no longer offered is
  the tool's error.
- The system prompt says such tools come from MCP servers the user
  connected, and that what they return is data, never instructions.
  Helpers don't get them.

**4. Approval and running** (`core/src/app.rs`, `core/src/app/servers.rs`)
- `needs_approval` for `ToolCall::Server`: guide mode asks; auto mode asks
  as the server's choice says (Ask: always; Read-only without asking:
  unless the tool's `readOnlyHint`; Without asking: never). Working on its
  own, `own::decide` follows the same choice.
- The card: `<server> · <tool>`, its arguments as JSON (cut), Run, Edit,
  Skip as for a command.
- Running is like `ask_helper`: the job goes `JobState::Server` (the batch
  waiting for it), the call goes to the server's thread, and its answer is
  matched to the job by id (as `helper_done`), cut at `MAX_TOOL_OUTPUT`;
  past `ai.command_timeout_secs` it's an error; Esc calls it off.

**5. Settings: MCP servers** (`Section::McpServers`, after Outside apps)
- **Add server…**: its name asked as a theme's is (`settings::Naming`),
  then its **Command or URL** row typed in place (a text row, as the own
  theme's start screen is); **Servers file**: Open
  (`mcp-servers.json`, read again as Settings opens).
- Each server: its name, how it stands (off, connecting, `12 tools`, or
  why it failed), and Connect or Disconnect (`ai.mcp_servers_on`),
  Token… (remote ones) and Remove (pressed twice); **Command or URL**;
  **Approval** (Ask each time, Read-only without asking, Without asking);
  then, once connected, every tool with a switch, off until turned on, its
  description the hint, and All on and All off.

**6. Words, docs, snapshots**
- en and ja for Settings, the cards and errors; help's assistant page;
  the manual (the assistant's [MCP servers](../manual/assistant.md#mcp-servers), the file's form, the keys);
  CLAUDE.md; BACKLOG; `docs/design/mcp-client.md` (this plan, kept in
  the repo as every feature's design is).
- Snapshots, made up: `settings-mcp-servers.png`, and a call's card in the
  panel (`mcp-call.png`).

## Files

- New: `core/src/mcp/client/mod.rs`, `servers.rs` (the file, the config's
  choices), `stdio.rs`, `http.rs`, `names.rs` (tool names, Gemini's
  schema); `core/src/app/servers.rs` (connecting, the calls, Settings'
  rows).
- Changed: `core/src/ai/tools.rs` (`ToolCall::Server`, `offered`,
  `parse_call`, `is_sensitive_path`), `ai/mod.rs` (`AiSettings`, the
  prompt), `ai/claude.rs`, `openai.rs`, `gemini.rs` (`offered`, Gemini's
  cleaning), `ai/own.rs`, `core/src/app.rs` (`needs_approval`,
  `start_job`, `advance_tools`, the key dialog), `secrets.rs` (named
  secrets), `core/src/config.rs`, `core/src/settings.rs`,
  `core/src/event.rs`, locales and docs.
- Reused: `provider::http_agent`, `read_sse`; the `ask_helper` job
  pattern (`JobState::Helping`, `helper_done`); `settings::Naming` and the
  text rows; the key dialog; `bridge::address`'s config folder.

## Tests

- `servers`: the file read in each client's form (stdio, `url`, `type`),
  `${env:…}`, an `sse` server refused; the choices from config.
- `names`: names for the model, shortened and unique; Gemini's schema
  cleaned; a call's name back to its server and tool.
- `stdio`: against the built `flow-mcp` and a stand-in Flow
  (`tests/mcp.rs`, which already has one): initialize, tools/list, a call,
  an error; a server that exits, its stderr in the error.
- `http`: a local mock server answering in JSON and in SSE, with a
  session id and a token header.
- The mock API: the turned-on tools offered to each provider, under their
  names, Gemini's cleaned, none to a helper.
- Approval: each choice in guide mode, auto mode and working on its own.
- Settings' section rows; i18n; `cargo test --workspace`; clippy at
  flow-core's 8.

## Verification

- Snapshots looked at.
- Live, in an isolated home: Flow's assistant connected to `flow-mcp`
  (Flow's own server, against a second Flow) or another stdio server, its
  tools listed, one turned on, called with a mock provider; then with a
  real key and a real server (Claude Desktop's own config pasted in).
- Windows and Linux build and try it from master afterwards.

## As built (the Mac, toward 0.1.42)

Built as planned. Where it went its own way:

- **Connecting** is `core/src/app/servers.rs` (`Servers`, `read_servers`,
  `follow_servers`, `server_event`), each server's thread a
  `mcp::client::Link`. A remote server's token is read on that thread
  (`mcp::client::Token`), so a Keychain prompt never holds up the window.
  A server that failed stays failed until Connect, or Flow's next start.
- **The tools offered:** `AiSettings::offered()` (Flow's own, then the
  servers' for the assistant alone), which each provider's `body()` takes;
  `SERVERS_PROMPT` joins the system prompt while there are any. Fixed on
  the way: a change in Settings configured the agent without the user's
  instructions and skills (`follow_config` now uses `agent_settings`).
- **A call** is `ToolCall::Server { name, args }`; `needs_approval` and
  `own_asks` (`own::Why::Server`) ask as the server's `Approval` says. It
  runs as `JobState::Serving { until }`, which counts as waiting (a look
  each second, no frames), answered by job id (`server_answered`), given
  up past the command timeout and a grace of 5 s. Esc drops the batch, and
  a late answer is let go; the server isn't told (no
  `notifications/cancelled` yet).
- **The card:** `<server> · <tool>`, its arguments on one line where they
  fit in 72 characters, else laid out (8 lines at most). **Edit** edits
  them as JSON in the input; what isn't an object of arguments is refused
  with a toast and stays being edited.
- **Settings:** `Section::McpServers` after Outside apps, its rows
  `ServersFile`, `Server`, `ServerHow`, `ServerApproval`, `ServerTools`,
  `ServerTool` (`settings::servers_rows`, `ServersView`, `server_acts`).
  Add server… is a `Naming` (`Named::Server`, its words `server_name.*`)
  whose name becomes a row of its own (`Servers::draft`) until its command
  or URL is typed; that row's typing is kept when it's done, not as each
  key comes (`server_line_typed`). Remove asks for a second press.
- **Token…** is the key dialog made to take a server's token
  (`KeyDialog::server_token`); an empty box forgets the one kept.
- `is_sensitive_path` now covers `mcp-servers.json` and `mcp.json`, and auto
  mode asks before writing any sensitive path, as working on its own does.
- Approval is tested where it's decided (`Approval::asks`); the App's
  wiring of it is tried by hand, not by a test (the App needs a window).
- A done card's line is `answer_summary`: a JSON answer on one line, else
  its first line with words, cut at 100 characters.
- Tried live in a test copy: Flow's assistant used Flow's own `flow-mcp`
  as a server, through both cards, end to end.

## Finishing it (0.1.43)

The plan agreed on 2026-11-16, kept here as every feature's design is.


### Context

0.1.42 shipped MCP both ways: `flow-mcp` (Flow as a server for outside
apps) and the assistant using MCP servers (`core/src/mcp/client/`,
`core/src/app/servers.rs`; design in `docs/design/mcp-client.md`). MCP
was to be finished first in 0.1.43 (decided 2026-11-16):

- **All four parts:** OAuth sign-in, resources and prompts, servers asking
  the user (elicitation), and the newest MCP revision (2025-11-25), both
  ways. Telling a server a call was given up (`notifications/cancelled`)
  comes too.
- **Registration:** dynamic client registration where the server's sign-in
  offers it, else a client ID the user types. Flow publishes nothing (no
  Client ID Metadata Document for now).
- **Resources:** the assistant reads them through tools. The user doesn't
  attach them with @.

Left out on purpose: sampling (a server spending the user's tokens),
roots, tasks (experimental), subscribing to a resource, a server's
`completion/complete`, and the optional GET stream outside a request.
Flow keeps refusing those requests, as it does today.

### 1. The link, made ready for the rest (`core/src/mcp/client/`)

Today a server's thread blocks on each `tools/call`, refuses every request
from the server but `ping`, and can't send anything while a call waits.
So:
- `Transport` takes `&self` (its state inside): `request(method, params,
  wait, cancel: &AtomicBool)` and `send(message)`, for notifications and
  for answering the server.
- Each call (and each resource read or prompt) runs on a thread of its own
  (`flow-mcp-call`). One that's given up never holds up the next.
- stdio's reader routes each answer to its waiting request by id (a map of
  senders in place of one channel). HTTP's posts are independent, and the
  session sits in a mutex.
- What the server sends unasked goes to the link's thread on an
  `incoming` channel: notifications, and requests. A `ping` is answered,
  `elicitation/create` goes to the window, and anything else is refused
  with -32601. On HTTP these come inside a POST's event stream.
- `initialize` sends `capabilities: { elicitation: { form: {}, url: {} } }`
  and `clientInfo.description`. It keeps the server's capabilities, so
  resources and prompts are listed only where the server has them.

**Cancelling.** `Link::cancel(job)` sends `notifications/cancelled
{ requestId, reason }` for that call's request (never for `initialize`),
and sets the call's cancel flag so its thread lets go. It's called from
`stop_ai` (Esc, Stop) and when a `JobState::Serving` call times out
(`advance_tools`). That timeout is the call's only clock: the link waits
an hour by itself (`SERVING_LONGEST`). Until 2026-11-18 it waited the
command timeout too, 5 s less than the window, so a call out of time
failed there first, the server untold.

### 2. MCP 2025-11-25, both ways

- `VERSIONS` in `mcp/client/mod.rs` and `mcp/protocol.rs` gain
  `"2025-11-25"`, newest first.
- **Picking a stream up again (client):** http.rs reads its streams with an
  SSE reader of its own that keeps each event's `id:` and a `retry:`
  delay (`provider::read_sse` drops both). A POST's stream that ends
  before the answer is resumed with a GET carrying `Last-Event-ID`, after
  the delay the server gave (1 s where it gave none). It tries again until
  the call's time is up, and gives up on a 405 or after five tries in a
  row with no new event.
- **flow-mcp (server):** bad arguments to its tools come back as a tool's
  error (`isError`), not a protocol error, as 2025-11-25 says. Its
  `serverInfo` gets a `description`.
- Icons and the newer schema fields are passed over.

### 3. Signing in (`core/src/mcp/client/oauth.rs`, new)

This follows the spec's Authorization page (2025-11-25).

1. **The ask.** A 401 (or a 403 `insufficient_scope`) from a remote server
   carries a `WWW-Authenticate` challenge. Flow parses its
   `resource_metadata`, `scope` and `error`.
2. **Standing.** The server then stands as **needs you to sign in**
   (`ServerEvent::NeedsSignIn`, `Standing::SignIn`). A toast points to
   Settings. Flow never opens the browser by itself.
3. **Settings' Sign in** runs the flow on a `flow-mcp-sign-in` thread:
   1. Discovery:
      - The protected resource metadata, from the challenge's URL, else
        `/.well-known/oauth-protected-resource/<path>`, then the root one.
      - Its first authorization server's metadata, in the spec's order:
        RFC 8414 with path insertion, then OIDC with insertion, then OIDC
        appended.
      - Refused where it lists no `S256` in `code_challenge_methods_supported`,
        its `issuer` doesn't match, or an endpoint isn't HTTPS (loopback
        hosts allowed, for tests and local servers), or where the
        metadata's `resource` doesn't cover the server's URL.
   2. Registration:
      - A client ID the user typed (Settings' **Client ID…**), else one
        Flow registered before.
      - Else dynamic registration at `registration_endpoint`:
        `client_name` Flow, `token_endpoint_auth_method` none (or the
        secret it's given back), authorization code and refresh token.
      - Else it fails, saying to type a client ID.
   3. The browser:
      - A loopback listener on `127.0.0.1`: the fixed `CALLBACK_PORT`
        first (which Settings' hint names, for registering a client ID by
        hand), else a free port, registering again for it.
      - The authorization URL carries PKCE S256, a random `state`,
        `resource` (the server's canonical URI) and the scope: the
        challenge's, else `scopes_supported`, else none.
      - The App opens it with `Platform::open_url` and a toast ("Finish
        signing in to <server> in your browser").
      - The listener waits up to five minutes. It checks `state` and
        answers with a small page in the user's language.
   4. The token request: code, `code_verifier`, `redirect_uri`,
      `resource`.
   5. Storage:
      - The refresh token, client and endpoints are kept in the
        credential store (`mcp-oauth-<server>`, split over numbered
        entries past Windows' 2,560-byte limit).
      - The access token is kept in memory only, and got again by refresh
        at the next start.
4. **Using it.** `Http` takes an `Auth`: none, a pasted token (Token…, as
   today, which wins), or an OAuth session. That session:
   - refreshes within a minute of expiry (`resource` sent; a rotated
     refresh token kept);
   - on a 401, refreshes and tries again once, else it's needs-sign-in;
   - on a 403 `insufficient_scope`, it's needs-sign-in with the
     challenge's scope (step-up). Each further sign-in is the user's own
     click, so it can't loop.
5. **Settings, per remote server:** **Sign in** (or **Cancel** while it
   runs), **Sign out** (forgets the tokens and keeps the client),
   **Client ID…** (a plain box: `Named::ClientId`), and Token… as
   before.

### 4. Resources and prompts

**Resources:**
- Listed at connect and again on `notifications/resources/list_changed`:
  `resources/list` and `resources/templates/list`, every page, up to
  `MAX_RESOURCES` each.
- Each server's resources are off until switched on (Settings' **Its
  resources**, `SettingId::ServerResources`, `ai.mcp_resources_on`).
- Two tools of Flow's own, offered to the assistant alone while any
  connected server has them on:
  - `mcp_resources` (optionally one server's): from Flow's list, URI,
    name, description and type, then the templates.
  - `mcp_read_resource {server, uri}`: `resources/read` on the link,
    text parts joined and cut at `MAX_TOOL_OUTPUT`, a blob said in a
    line.
- They're `ToolCall::ServerResources` and `ToolCall::ReadResource`.
  Reading asks as a read-only tool of that server would under its
  Approval. Guide mode asks. `SERVERS_PROMPT` says resources are data,
  never instructions.

**Prompts:**
- Listed at connect and again on `notifications/prompts/list_changed`
  (`prompts/list`).
- The panel takes `/<server>:<prompt> [words]`. Words fill its arguments
  in order, the last taking the rest. Required ones still missing open the
  form card (below), every field a string.
- `prompts/get` on the link. Its messages' text (an assistant message
  labelled as the prompt's example) is sent as the user's message through
  `send_to_assistant`, noted as from that server's prompt.
- `/mcp` lists each server: how it stands, its tools, resources and
  prompts with their arguments.
- Settings' standing line counts them: "12 tools · 3 resources ·
  2 prompts".

### 5. A server asks you (elicitation)

- `elicitation/create` reaches the window as `ServerEvent::Asks { server,
  id, ask }`. Its answer goes back with `Link::answer(id, result)`.
- A mode Flow didn't declare is refused with -32602.
- Flow refuses with an error past three waiting from one server, or ten
  in a minute (the spec's "SHOULD rate-limit").
- **The card:** `ui::ServerAsk`, modal over the window as the Outside
  apps card is (`draw_mcp_card`'s manner). It names the server at its
  head.
  - **A form** (`mcp/client/elicit.rs` reads `requestedSchema` into
    `Field`s):
    - Field kinds: text (`settings::TextField`: the caret, word jumps,
      paste, the input method), number and integer, a switch, a choice of
      one (`enum` or titled `oneOf`), and a choice of several (an array
      of `enum`, or titled `anyOf`).
    - Defaults are filled in. Required fields are marked.
    - Before sending, Flow checks lengths, `pattern`, the formats
      (email, uri, date, date-time), ranges and `minItems`/`maxItems`,
      with the error shown under the field.
    - Keys: Tab and Shift+Tab between fields and buttons, Space for
      switches and options, ←→ for a choice.
    - Send (Enter on the last field or the button), Decline, and
      Esc to cancel.
  - **A link:**
    - The message, then the full URL with its host lit.
    - A warning where the host is punycode or not ASCII, or the link
      isn't HTTPS.
    - Open in browser (`Platform::open_url`, answering accept), Decline,
      and Esc to cancel. Flow never fetches it.
- **-32042 (`URLElicitationRequiredError`)** on a call puts up its link
  cards in turn. Once opened:
  - The call's card in the panel says it waits for the browser, with
    **Try again** and **Give up** (`Hit::CardRetry`, `Hit::CardGiveUp`).
  - `notifications/elicitation/complete` for those ids, or Try again,
    calls the tool once more. It gives up after ten minutes.
- While a call's server waits on the user, the call's time stands still
  (`Serving`'s `until` kept a whole command timeout ahead while the card
  shows, so the call has all of it again once the user has answered).
- Working on its own, an ask always waits for the user, and the system
  notifies when Flow is behind (`notify_assistant`).
- What's typed goes to the server alone: never to the model, never
  journaled.
- Readers get it as a dialog (`DialogKind::ServerAsk`, its fields as
  controls). F1 shows a help page of its own (`help::Page::ServerAsk`).

### Files

New:
- `core/src/mcp/client/oauth.rs`: discovery, registration, PKCE, the
  listener, tokens.
- `core/src/mcp/client/elicit.rs`: schema to fields, checking, content.
- `core/src/mcp/client/sse.rs`: SSE with ids and retry.

Changed:
- `mcp/client/mod.rs`, `stdio.rs`, `http.rs`, `servers.rs`
  (`ai.mcp_resources_on`), and `mcp/protocol.rs` with flow-mcp's tools.
- `core/src/app/servers.rs`: sign-in, resources, prompts, asks, cancel.
- `core/src/app.rs`: `stop_ai`, `advance_tools`, `submit_ai`'s
  `/server:prompt` and `/mcp`, the card's keys.
- `ai/tools.rs`, `ai/mod.rs` (`offered`, `SERVERS_PROMPT`), `ai/own.rs`.
- `secrets.rs` (named secrets, split), `config.rs`, `settings.rs`,
  `ui.rs`, `access.rs`, `help.rs`, `event.rs`.
- en and ja.
- `core/Cargo.toml`: `getrandom`, `regex` and `percent-encoding`, all
  three already in Cargo.lock.

Reused: `settings::TextField`, `Naming`, the key dialog, `draw_mcp_card`'s
look, `Platform::open_url`, `names::for_model`, `answer`, `result_of`, and
`http.rs`'s test server (`serve`).

Docs:
- The manual's [MCP servers](../manual/assistant.md#mcp-servers) section and
  [config keys](../manual/configuration.md); CLAUDE.md's "The
  assistant's MCP servers".
- BACKLOG: a 0.1.43 section.
- `docs/design/mcp-client.md`: a 0.1.43 part with this plan.

### Tests

- **oauth:**
  - `WWW-Authenticate` parsing (quoted values, several schemes).
  - The well-known URLs in the spec's orders.
  - PKCE against RFC 7636's own vector.
  - The canonical URI.
  - Metadata refused: no S256, a wrong issuer, plain HTTP.
  - The whole flow against a stand-in sign-in server and MCP server on
    127.0.0.1, the "browser" fetching the URL: dynamic registration,
    state, the token request's fields, then refresh with rotation, a 401
    refreshed and retried, a 403 step-up.
- **elicit:** each field kind from its schema; defaults; every check;
  the content sent; a link's host lit and punycode warned of.
- **The link (an HTTP stand-in):**
  - An `elicitation/create` inside a call's stream, answered by POST
    before the call's answer.
  - `notifications/cancelled` carrying the call's id.
  - A dropped stream resumed by GET with `Last-Event-ID`.
  - Resources and prompts listed and read.
  - A call given up never holding the next.
- **stdio:** routing by id with two calls at once.
- **flow-mcp** at 2025-11-25 (`tests/mcp.rs`), with bad arguments as
  `isError`.
- **The rest:** the tools offered (resources only when on, never to
  helpers); `/server:prompt` parsing; Settings' rows; i18n;
  `cargo test --workspace`; clippy at flow-core's 8.

### Verification

- Snapshots looked at:
  - `settings-mcp-servers.png` again, made up with a server needing
    sign-in and resources on.
  - New: `mcp-ask-form.png` and `mcp-ask-link.png`.
- Live on the Mac, in the test copy in a scratch HOME, with the MCP
  project's reference `server-everything` (downloaded through npx), which
  has resources, prompts and elicitation:
  - a prompt run from the panel;
  - a resource read by the assistant;
  - a form and a link answered;
  - Esc on a long call, with the server told.
- OAuth live: a real remote server (Linear, Notion or Sentry offer
  dynamic registration), signed in to in one's own browser; Flow never
  types credentials.
- Then a try by hand, and Claude Desktop with `flow-mcp`.
- Windows and Linux build and try it from master afterwards.

### As built (the Mac, toward 0.1.43)

Built as planned. Where it went its own way:

- **The link** (`mcp/client/mod.rs`): a request's failure is a `Fail`
  (`Said`, `Gone`, `SignIn`, `Links`, `Stopped`), and an answer that comes
  after all to a request given up is let go as `Stopped`. Requests share a
  `Live` (the transport, the ids, `underway`); the link's thread `select!`s
  over Flow's asks and what the server sends unasked (`route_unasked`).
- **Streams** are read by `mcp/client/sse.rs` (`Events`, keeping ids and
  `retry:`), not `provider::read_sse`.
- **Signing in:** the client ID typed and its port live in
  `mcp-servers.json` (`oauth.clientId`, `oauth.callbackPort`, as Claude
  Code writes them), not config.toml. Where a server has no protected
  resource metadata, its origin is taken as the authorization server, as
  the spec's earlier versions had it. The kept sign-in is JSON split over
  credential entries of 1,000 characters (`secrets::store_long`). A
  pasted token wins over a sign-in; a refusal of a pasted token (or of the
  file's own `Authorization` header) is the user's to fix, not a sign-in.
- **Settings:** a remote server's sign-in has a row of its own
  (`SettingId::ServerAuth`: Sign in, Cancel or Sign out, Client ID…,
  Token…), so the server's row keeps Connect or Disconnect and Remove.
  Its standing line doesn't count resources and prompts after all (it
  didn't fit beside its buttons): the resources' row's hint gives their
  number, and the server's hint names its prompts.
- **Forms** keep the server's order because core turns on serde_json's
  `preserve_order` (no new crate: `indexmap` and `foldhash` were in the
  lock); two tests that compared JSON keys in sorted order compare them
  as sets now. `pattern` is checked with the `regex` crate, passed over
  where it isn't Rust's syntax.
- **Prompts' arguments** ask on the same card as a server's form, every
  field a string, the words typed after the command filled in.
- **flow-mcp** already gave bad arguments back as the tool's error; only
  its versions and `serverInfo.description` changed.
- **Tried live** against the MCP project's reference server
  (`server-everything` 2.3.0), headless (an ignored test,
  `FLOW_MCP_EVERYTHING`) and in a test copy of Flow.app. What that turned
  up, now fixed:
  - Settings' text field took 200 characters a line, which cut a command
    with its paths short: commands, URLs and a server's form answers take
    4,000 (`TextField::long`).
  - Add server… while Settings was searching hid the new row, so its
    command couldn't be typed: adding a server ends the search and opens
    the section.
  - A prompt was got with no API key, and its text lost: without one the
    panel says so first.
  - A call's time ran on while its server's question waited for the user:
    a call waits as long as a card of its server's is up.
  - A server's long answer was cut to its end, as a command's output is,
    so a file read lost its start: answers and resources keep their start
    (`truncate_head`).
  - What the user answers goes to the server alone, but the reference
    server echoed it in its tool's result: the words now say Flow sends
    it to the server alone, and the server may hand it back.


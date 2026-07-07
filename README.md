# gptbrowser

Prompt a logged-in chatbot web UI the way you'd call an API. `gptbrowser` drives your
already-open, already-authenticated Chrome through [PinchTab](https://github.com/) and
hands back clean, HTML-free text — no API key, no per-token billing, no scraping code
to babysit.

```bash
$ gptbrowser claude.ai "what's the capital of France?"
Paris.
```

One process, one tool call, one string back. That's the whole interface. Point it at
ChatGPT, Gemini, Kimi, DeepSeek, Claude, Perplexity, or z.ai and it navigates to the
real chat app, injects your prompt into that site's composer, waits for generation to
finish, and prints the final answer — deterministically, not by guessing a sleep
duration.

This matters most for **agents**: a sub-agent that needs to consult a chatbot web UI
can shell out to `gptbrowser` instead of driving a browser itself. See
[BENCHMARKS.md](BENCHMARKS.md) for what that's worth in practice (roughly 10–50x fewer
tokens than direct browser automation for the same task).

## Install

Prerequisites:

- [Rust](https://rustup.rs) (for `cargo build --release`)
- A running [PinchTab](https://github.com/) daemon, with Chrome already logged into
  whichever sites you plan to use
- `~/.pinchtab/config.json` present, containing PinchTab's `server.token` (and
  optionally `server.port`, default `9867`)

```bash
git clone <this repo> gpt-browser-automator
cd gpt-browser-automator
./build.sh
```

`build.sh` runs `cargo build --release` and installs the binary to
**`/opt/homebrew/bin/gptbrowser`** — not `~/.cargo/bin` — specifically so that
non-interactive shells and other tools/sub-agents that don't source your cargo env
still find it on `PATH`. Override the install location with `PREFIX`:

```bash
PREFIX=/usr/local/bin ./build.sh
```

Verify:

```bash
gptbrowser --help
```

If `gptbrowser: command not found` shows up from a sub-agent or script even though it
works in your interactive shell, that's almost always this — the binary landed
somewhere not on that process's `PATH`.

## Quick start

```bash
# positional: <url> <prompt...>
gptbrowser chatgpt.com "explain CAP theorem in two sentences"

# explicit flags
gptbrowser --url gemini.google.com --prompt "summarize this repo's README in one line"

# prompt via stdin (handy for piping in file contents)
git diff | gptbrowser claude.ai "review this diff for bugs"

# ask which models are available, then pick one
gptbrowser --list-models claude.ai
gptbrowser --model Opus claude.ai "which model are you?"

# structured output instead of bare text
gptbrowser --json perplexity.ai "latest stable Rust version?"
```

The URL only needs to identify the *site* — `gptbrowser` always navigates to that
site's canonical chat app URL regardless of what you typed (e.g. `gemini.com` — the
crypto exchange — correctly resolves to `gemini.google.com/app`, the actual Gemini
chat UI).

## Supported sites

| Site | Match on | App URL | Status |
|---|---|---|---|
| ChatGPT | `chatgpt`, `chat.openai` | `chatgpt.com/` | Working (logged in) |
| Gemini | `gemini` | `gemini.google.com/app` | Working (logged in) |
| Kimi | `kimi` | `www.kimi.com/` | Working (logged in) |
| DeepSeek | `deepseek` | `chat.deepseek.com/` | Working (logged in) |
| Claude | `claude` | `claude.ai/new` | Working (logged in) |
| Perplexity | `perplexity` | `www.perplexity.ai/` | Working (logged in) |
| z.ai | `z.ai` | `chat.z.ai/` | Working (guest OK) |
| Copilot | `copilot` | `copilot.microsoft.com/` | Login-gated — MS SSO wall blocks unattended use |
| Grok | `grok` | `grok.com/` | Login-gated — signup paywall blocks unattended use |

"Login-gated" sites have working adapters but currently return a clear
`are you logged in?`-style error until the account is authenticated in the
PinchTab-controlled Chrome profile.

Site detection is a simple substring match on the URL you pass — see
`Site::detect` in `src/main.rs` if you need the exact rule.

## Model switching

Pass `--model <name>` (fuzzy, case/space-insensitive match) to switch before the
prompt is sent, or `--list-models` to print what's currently selectable and exit.

| Site | Switchable models |
|---|---|
| Gemini | 3.1 Flash-Lite, 3.5 Flash, 3.1 Pro |
| Kimi | K2.6 Instant, Thinking, Agent, Agent Swarm |
| Claude | Opus 4.8, Sonnet 5, Haiku 4.5, Fable 5 (+ legacy models under "More models") |
| DeepSeek | DeepThink / Search (independent toggles, not a picker) |
| ChatGPT, Perplexity, z.ai | Gated behind a paid tier or guest/free-tier restrictions — `--list-models` will report an empty picker |

```bash
gptbrowser --list-models kimi.com
gptbrowser --model "Agent Swarm" kimi.com "plan a 3-step research task"
gptbrowser --model Haiku claude.ai "which model are you?"
# -> "Claude Haiku 4.5."
```

## How it works

1. **Client** — a `ureq`-based HTTP client talks to the local PinchTab daemon
   (`http://localhost:<port>`, Bearer token) at `/tabs`, `/tab`, and
   `/tabs/<id>/{evaluate,navigate,action}`. Config comes from `~/.pinchtab/config.json`
   (`server.token`, `server.port`), overridable with `PINCHTAB_URL` / `PINCHTAB_TOKEN`.
2. **Acquire a tab** — reuse an existing tab already on the target site, or open a new
   one (`--new-tab` forces a fresh tab). Focus it and navigate to the site's canonical
   app URL.
3. **Wait for the composer** — poll a per-site CSS selector (e.g. `#prompt-textarea`
   for ChatGPT, `.ql-editor` for Gemini, `div[data-testid="chat-input"]` for Claude)
   until it exists. If it never appears, that's the "are you logged in?" failure mode.
4. **Inject the prompt** — the composer implementation varies by site, so injection
   does too:
   - **ProseMirror/Quill/tiptap** (ChatGPT, Gemini, Claude): focus, `execCommand`
     `selectAll` + `delete`, then `insertText`.
   - **Lexical** (Kimi, Perplexity): `execCommand('delete')` no-ops on these editors,
     so instead select all contents via a DOM `Range` and let `insertText` replace it.
     Kimi's editor also starts `contenteditable=false` and needs a real click to
     activate — this is wrapped in an 8-attempt retry loop that clicks, waits, and
     re-checks injected length until it lands.
   - **Real `<textarea>`** (DeepSeek, z.ai, Grok, Copilot): React's native
     `HTMLTextAreaElement` value setter + a synthetic `input` event (bypasses React's
     controlled-input tracking).
5. **Submit** — click the site's send button where one exists and isn't disabled
   (ChatGPT, Gemini, Claude, Perplexity, z.ai, Grok); otherwise dispatch a synthetic
   `Enter` keydown/keypress/keyup sequence (Kimi, DeepSeek, Copilot).
6. **Foreground the tab, every poll.** This is the load-bearing reliability trick:
   Chrome throttles background tabs, which stalls the streamed SSE response mid-
   sentence. `gptbrowser` calls `POST /tab {action: focus}` on every single poll
   during generation, not just once at the start.
7. **Detect completion** — poll the latest assistant message's `innerText` length
   (whitespace stripped) plus a "currently generating" signal where the site exposes
   one (a visible stop button, for ChatGPT/Claude). Done = length hasn't grown for N
   consecutive polls **and** the site isn't reporting active generation. Defaults:
   poll every 700ms, 6 stable polls (~4.2s) to confirm, 45s to see the first token, 240s
   hard ceiling on total generation time.
8. **Extract and return** clean `innerText` — no HTML, no markdown fencing artifacts
   beyond what the site itself renders as plain text.

An MCP server that wraps this same flow as `ask_model` / `list_models` tools lives in
`mcp/` — see `mcp/README.md`.

## Troubleshooting

**`composer never appeared on <site> — are you logged in?`**
The composer selector never showed up within 30s. Open the site manually in the
PinchTab-controlled Chrome and confirm you're signed in; some sites (Copilot, Grok)
sit behind a login/signup wall that blocks unattended use entirely.

**`request failed: ... (is the PinchTab daemon running?)`**
The HTTP call to the local PinchTab daemon failed outright. Start/restart the daemon
and confirm `~/.pinchtab/config.json` has a valid `server.token` and port (or set
`PINCHTAB_URL` / `PINCHTAB_TOKEN` directly).

**`prompt did not land in <site> composer (got N chars, expected ~M)`**
Injection landed but partially, usually because the site changed its composer's DOM
(selector drift) or the editor needs an interaction pattern `gptbrowser` doesn't yet
handle. Compare the site's live DOM against the relevant `composer_sel()` /
`inject_js()` arm in `src/main.rs`.

**`<site> produced no response within 45s of sending`**
Either the send action didn't register (check `send_button_sel()` /
`enter_submit_js()` for that site) or the site is just slow to start generating —
raise the ceiling with `--timeout` if needed, though this specific error is bounded by
the fixed 45s first-token wait, not `--timeout`.

**Response looks truncated / warning about hitting the timeout**
`gptbrowser` prints `warning — hit <N>s timeout; returning partial response` to
stderr and returns whatever it has. Raise `--timeout`, or if it's happening on every
call, the "stable" detector may be tripping on a site quirk — try `--stable` with a
higher count.

**Model switching fails with "not found" / "not selectable"**
The error lists the models the picker actually offered. Free/guest tiers on ChatGPT,
Perplexity, and z.ai often gate model choice — `--list-models` will show an empty or
restricted set in that case.

## More

- [BENCHMARKS.md](BENCHMARKS.md) — measured token/tool-call/latency comparison against
  driving PinchTab directly.
- [docs/USAGE.md](docs/USAGE.md) — using `gptbrowser` from Claude Code (slash command +
  MCP), Codex/other shells, and Antigravity/any MCP client.
- `mcp/README.md` — the MCP server that exposes `ask_model` / `list_models` to any MCP
  client.

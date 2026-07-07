# gptbrowser

> Prompt a logged-in chatbot web UI the way you'd call an API — no API key, no per-token bill, no scraping code to babysit.

`gptbrowser` is a single, dependency-light Rust binary that drives your **already-open, already-authenticated Chrome** through [PinchTab](#what-is-pinchtab) and hands back clean, HTML-free text. Point it at ChatGPT, Gemini, Kimi, DeepSeek, Claude, Perplexity, or z.ai; it navigates to the real chat app, injects your prompt into that site's composer, waits for generation to *actually* finish, and prints the final answer.

```console
$ gptbrowser claude.ai "what's the capital of France?"
Paris.
```

One process, one call, one string back. That's the whole interface.

It exists to make chatbot web UIs usable **by other programs** — especially coding agents. A sub-agent that needs a second opinion from Gemini can shell out to `gptbrowser` instead of driving a browser itself, at roughly **10–50× fewer tokens** for the same task ([benchmarks](#benchmarks)).

---

## Table of contents

- [Why this exists](#why-this-exists)
- [Features](#features)
- [Supported sites](#supported-sites)
- [How it works](#how-it-works)
- [What is PinchTab?](#what-is-pinchtab)
- [Prerequisites](#prerequisites)
- [Installation](#installation)
- [Configuration](#configuration)
- [CLI reference](#cli-reference)
- [Model switching](#model-switching)
- [Image generation](#image-generation)
- [Using it inside coding tools](#using-it-inside-coding-tools)
  - [Claude Code](#claude-code)
  - [Codex CLI](#codex-cli)
  - [Antigravity](#antigravity)
  - [Cursor / Windsurf / Zed / generic MCP client](#cursor--windsurf--zed--generic-mcp-client)
  - [Plain shells & CI](#plain-shells--ci)
- [The MCP server](#the-mcp-server)
- [Benchmarks](#benchmarks)
- [Knowledge graph](#knowledge-graph)
- [Project layout](#project-layout)
- [Troubleshooting](#troubleshooting)
- [Building from source](#building-from-source)
- [Security notes](#security-notes)
- [Limitations & roadmap](#limitations--roadmap)
- [Contributing](#contributing)
- [License](#license)

---

## Why this exists

Chatbot web UIs are gated behind logins, bot-detection, and (often) subscriptions you're *already paying for* in the browser. Hitting the paid API is a second bill; scraping the DOM by hand is brittle and burns enormous token budgets when an LLM agent does it live.

`gptbrowser` collapses all of that into a Unix-shaped tool:

- **You're already logged in.** It reuses your real Chrome session via PinchTab — your ChatGPT Plus / Gemini Advanced / Claude Pro subscription, your history, your rate limits. No API key.
- **It's deterministic.** Completion is detected by watching the response text stabilize plus a per-site "is it still generating?" signal — not a hard-coded `sleep`.
- **It's cheap for agents.** The output is plain text. A sub-agent pays for one `execFile` call and one text blob back, not dozens of DOM-dump round trips.
- **It's one binary.** `ureq` + `serde_json`, no TLS stack, no headless-browser download. Drop it on `PATH` and every tool on the machine can call it.

---

## Features

- **9 site adapters** — ChatGPT, Gemini, Kimi, DeepSeek, Claude, Perplexity, z.ai (working), plus Copilot & Grok (adapters present, login-gated).
- **Deterministic completion detection** — stall-based, with a generating-signal guard; tunable via `--stable` / `--poll` / `--timeout`.
- **Model switching** — `--model <name>` (fuzzy match) and `--list-models` on Gemini, Kimi, Claude, and DeepSeek.
- **Image generation** — `--image` drives ChatGPT's image tool and downloads the result to a file.
- **Three interfaces** — raw CLI, a `/gptbrowser` Claude Code slash command, and a stdio **MCP server** (`ask_model` / `list_models`).
- **Structured or bare output** — `--json` for scripting, plain text by default.
- **Stdin support** — pipe file contents or diffs straight in.
- **Zero heavy deps** — `ureq` (no-TLS) + `serde_json`; release binary is LTO'd and stripped.

---

## Supported sites

| Site | Match on (substring) | Canonical app URL | Status |
|---|---|---|---|
| ChatGPT | `chatgpt`, `chat.openai` | `chatgpt.com/` | ✅ Working (logged in) |
| Gemini | `gemini` | `gemini.google.com/app` | ✅ Working (logged in) |
| Kimi | `kimi` | `www.kimi.com/` | ✅ Working (logged in) |
| DeepSeek | `deepseek` | `chat.deepseek.com/` | ✅ Working (logged in) |
| Claude | `claude` | `claude.ai/new` | ✅ Working (logged in) |
| Perplexity | `perplexity` | `www.perplexity.ai/` | ✅ Working (logged in) |
| z.ai | `z.ai` | `chat.z.ai/` | ✅ Working (guest OK) |
| Copilot | `copilot` | `copilot.microsoft.com/` | ⚠️ Login-gated (MS SSO wall) |
| Grok | `grok` | `grok.com/` | ⚠️ Login-gated (signup paywall) |

> **You only pass the *site*, not the exact URL.** Detection is a substring match, and `gptbrowser` always navigates to that site's canonical chat app. Notably `gemini.com` (the crypto exchange) correctly resolves to `gemini.google.com/app` (the actual Gemini chat UI). The exact rule is `Site::detect` in [`src/main.rs`](src/main.rs).

"Login-gated" sites have working adapters but return a clear `are you logged in?`-style error until the account is authenticated in the PinchTab-controlled Chrome profile.

---

## How it works

```
   ┌────────────┐   execFile / stdin    ┌──────────────┐   HTTP + Bearer    ┌───────────────┐   CDP    ┌─────────┐
   │ your agent │ ────────────────────▶ │  gptbrowser  │ ─────────────────▶ │   PinchTab    │ ───────▶ │ Chrome  │
   │ or shell   │ ◀──────────────────── │  (Rust bin)  │ ◀───────────────── │  daemon :9867 │ ◀─────── │ (you)   │
   └────────────┘     clean text         └──────────────┘   {"result": …}    └───────────────┘          └─────────┘
```

Per call, `gptbrowser`:

1. **Reads config** from `~/.pinchtab/config.json` (`server.token`, `server.port`) — overridable with `PINCHTAB_URL` / `PINCHTAB_TOKEN`.
2. **Acquires a tab** — reuses one already on the target site, or opens a new one (`--new-tab` forces fresh). Focuses it and navigates to the canonical app URL.
3. **Waits for the composer** — polls a per-site CSS selector (e.g. `#prompt-textarea` for ChatGPT, `.ql-editor` for Gemini, `div[data-testid="chat-input"]` for Claude). Never appearing ⇒ the "are you logged in?" failure.
4. **Injects the prompt** using the right strategy for that editor:
   - **ProseMirror / Quill / tiptap** (ChatGPT, Gemini, Claude): focus → `execCommand` selectAll+delete → `insertText`.
   - **Lexical** (Kimi, Perplexity): delete no-ops here, so select contents via a DOM `Range` and let `insertText` replace. Kimi's editor starts `contenteditable=false` and needs a real click first — wrapped in an 8-attempt retry loop.
   - **Real `<textarea>`** (DeepSeek, z.ai, Grok, Copilot): React's native value setter + a synthetic `input` event.
5. **Submits** — clicks the send button where one exists and is enabled; otherwise dispatches a synthetic `Enter` key sequence.
6. **Foregrounds the tab on every poll.** This is the load-bearing reliability trick: Chrome throttles background tabs, which stalls the streamed SSE response mid-sentence. `gptbrowser` re-focuses the tab on *every* poll during generation.
7. **Detects completion** — polls the assistant message's `innerText` length (whitespace-stripped) plus a "currently generating" signal where the site exposes one. Done = length stable for N polls **and** not generating.
8. **Returns clean `innerText`** — no HTML, no fencing artifacts beyond what the site renders as plain text.

**Tunable defaults** (constants at the top of `src/main.rs`):

| Constant | Default | Meaning |
|---|---|---|
| `DEFAULT_POLL_MS` | `700` | how often response length is sampled |
| `DEFAULT_STABLE_POLLS` | `6` (~4.2 s) | no-growth samples in a row ⇒ "done" |
| `FIRST_TOKEN_TIMEOUT_S` | `45` | how long to wait for generation to *start* |
| `DEFAULT_TOTAL_TIMEOUT_S` | `240` | hard ceiling on one generation |
| `COMPOSER_TIMEOUT_S` | `30` | how long to wait for the page/composer to load |

---

## What is PinchTab?

**PinchTab** is a local browser bridge: it attaches to your running Chrome and exposes an HTTP API (default `http://localhost:9867`, Bearer-token auth) for listing tabs, navigating, evaluating JavaScript, and dispatching real (compositor-level) clicks. `gptbrowser` is a *client* of that API — it never launches its own browser, so everything runs inside your real, logged-in session.

You need the PinchTab daemon running with your config at `~/.pinchtab/config.json` before `gptbrowser` will do anything. See [Configuration](#configuration).

---

## Prerequisites

- **[Rust](https://rustup.rs)** (stable) — to build the binary.
- **Google Chrome**, open and **logged in** to whichever sites you plan to query.
- **A running PinchTab daemon**, with `~/.pinchtab/config.json` present and containing `server.token` (and optionally `server.port`, default `9867`).
- **Node.js ≥ 18** — *only* if you want the MCP server (`mcp/`).

---

## Installation

### 1. Clone & build

```bash
git clone https://github.com/anantbhandarkar/gpt-browser-automator.git
cd gpt-browser-automator
./build.sh
```

`build.sh` runs `cargo build --release` and installs the binary to **`/opt/homebrew/bin/gptbrowser`**.

> **Why `/opt/homebrew/bin` and not `~/.cargo/bin`?**
> `~/.cargo/bin` is **not** on the bare, non-interactive `PATH` that sub-agents, MCP servers, and CI steps inherit — so a tool that works in your interactive shell would fail with `command not found` when an agent calls it. `/opt/homebrew/bin` is already on `PATH` for GUI apps and non-interactive shells on macOS. This one detail is the single most common cause of "it works for me but not for the agent."

Override the install prefix if you like:

```bash
PREFIX=/usr/local/bin ./build.sh      # Linux / Intel mac
PREFIX="$HOME/.local/bin" ./build.sh  # user-local, if it's on PATH
```

### 2. Verify

```bash
gptbrowser --help
gptbrowser claude.ai "say hi in one word"
```

If the second command prints a one-word reply, you're done.

### 3. (Optional) Build the MCP server

```bash
cd mcp
npm install
npm run build     # produces dist/index.js
```

### Updating

```bash
cd gpt-browser-automator
git pull
./build.sh        # rebuild + reinstall over the old binary
```

---

## Configuration

`gptbrowser` needs to know **where** the PinchTab daemon is and **what token** to present. Resolution order:

1. **Environment variables** (highest priority): if both `PINCHTAB_URL` and `PINCHTAB_TOKEN` are set, they win.
2. **`~/.pinchtab/config.json`** — reads `server.token` (required) and `server.port` (default `9867`). The base URL defaults to `http://localhost:<port>` unless `PINCHTAB_URL` overrides it.

```jsonc
// ~/.pinchtab/config.json
{
  "server": {
    "token": "your-pinchtab-token-here",
    "port": 9867
  }
}
```

| Variable | Purpose | Default |
|---|---|---|
| `PINCHTAB_URL` | Base URL of the PinchTab daemon | `http://localhost:<port>` |
| `PINCHTAB_TOKEN` | Bearer token | from `~/.pinchtab/config.json` |
| `GPTBROWSER_BIN` | *(MCP server only)* absolute path to the `gptbrowser` binary | `gptbrowser` (via `$PATH`) |

Environment overrides are the clean way to point an agent or CI job at a non-default daemon without touching the config file.

---

## CLI reference

```
gptbrowser <url> <prompt...>
gptbrowser --url <url> --prompt "<text>"
echo "<prompt>" | gptbrowser <url>
```

The prompt can come from **positional args**, `--prompt`, or **stdin** — whichever is present. The first positional token is always the site URL.

### Flags

| Flag | Argument | Description |
|---|---|---|
| `--url` | `<url>` | Site to target (alternative to the positional URL). |
| `--prompt` | `<text>` | Prompt text (alternative to positional / stdin). Repeatable; parts are joined. |
| `--model` | `<name>` | Switch model before sending (fuzzy, case/space-insensitive). See [Model switching](#model-switching). |
| `--list-models` | — | Print the site's selectable models and exit. |
| `--image` | — | Generate an image (**ChatGPT only**) and download it; prints the saved file path. |
| `--out` | `<path>` | Output file for `--image` (default `gptbrowser-image-<epoch>.png`). |
| `--json` | — | Emit structured JSON instead of bare text. |
| `--new-tab` | — | Force a new tab instead of reusing an open one. |
| `--timeout` | `<sec>` | Overall generation budget (default `240`). |
| `--poll` | `<ms>` | Poll interval while generating (default `700`). |
| `--stable` | `<n>` | No-growth polls that mark completion (default `6`). |
| `-h`, `--help` | — | Print help and exit. |

### Output

**Default:** the response text, verbatim, on stdout.

**`--json`:** a stable object —

```json
{
  "model": "chatgpt",
  "url": "https://chatgpt.com/",
  "prompt": "ping",
  "response": "pong! how can I help?",
  "elapsed_ms": 8421
}
```

For `--image`, the JSON has `image_path` instead of `response` (and bare mode prints just the path).

### Exit codes

- **`0`** — success; response on stdout.
- **non-zero** — failure; a `gptbrowser: error: <reason>` line on **stderr**. Safe to check the normal shell way:

```bash
if ! out=$(gptbrowser claude.ai "$PROMPT" 2>err.log); then
  echo "gptbrowser failed: $(cat err.log)" >&2
  exit 1
fi
```

### Examples

```bash
# positional
gptbrowser chatgpt.com "explain CAP theorem in two sentences"

# explicit flags
gptbrowser --url gemini.google.com --prompt "summarize this repo's README in one line"

# stdin (pipe a diff in)
git diff | gptbrowser claude.ai "review this diff for bugs"

# structured output, extract one field
gptbrowser --json perplexity.ai "latest stable Rust version?" | jq -r .response

# a slower model — raise the ceiling
gptbrowser --timeout 360 --model "3.1 Pro" gemini.com "prove there are infinitely many primes"
```

---

## Model switching

Pass `--model <name>` (fuzzy match) to switch before the prompt is sent, or `--list-models` to print what's currently selectable and exit.

| Site | Switchable models |
|---|---|
| **Gemini** | 3.1 Flash-Lite, 3.5 Flash, 3.1 Pro |
| **Kimi** | K2.6 Instant, Thinking, Agent, Agent Swarm |
| **Claude** | Opus 4.8, Sonnet 5, Haiku 4.5, Fable 5 (+ legacy under "More models") |
| **DeepSeek** | DeepThink / Search (independent toggles, not a picker) |
| ChatGPT, Perplexity, z.ai | Gated behind a paid tier or guest/free restrictions — `--list-models` reports an empty/restricted set |

```bash
gptbrowser --list-models kimi.com
gptbrowser --model "Agent Swarm" kimi.com "plan a 3-step research task"
gptbrowser --model Haiku claude.ai "which model are you?"
# -> "Claude Haiku 4.5."
```

Under the hood the switch engine marks the target option element, then issues a **real** PinchTab click (needed for Angular/Gemini) with a **synthetic-pointer fallback** (needed for Claude's fixed-header button and Kimi's `div`), verifying the menu actually opened before selecting. Note: Gemini's "3.1 Pro" (thinking) can exceed the 45 s first-token window — raise `--timeout`.

---

## Image generation

`--image` (ChatGPT only) prepends an image-generation instruction to your prompt, disables image-blocking so the result renders, waits for the generated `<img>`, downloads it, and prints the saved path.

```bash
gptbrowser --image --out diagram.png chatgpt.com \
  "a clean flat-vector architecture diagram: a terminal, a browser, and three chat bubbles; slate + teal, simple line icons"
# -> diagram.png
```

Download handles both shapes ChatGPT serves:
- **signed `https://…oaiusercontent.com/…` URLs** → fetched as raw bytes through PinchTab's `/download?raw=true` (carries session cookies) and written from Rust.
- **in-memory `blob:` URLs** → read in-page via `fetch` + `FileReader` and base64-decoded with a tiny built-in decoder (no extra crates).

> **Heads-up:** ChatGPT's own free tier has a **daily image cap**. When it's hit, ChatGPT returns "Image generation failed" and `gptbrowser` surfaces that as a clean error rather than hanging. A working file is produced the moment the cap resets or on a Plus account.

---

## Using it inside coding tools

The binary is the same everywhere; only the *calling convention* changes. Pick your tool.

### Claude Code

**Option A — the `/gptbrowser` slash command** (simplest):

```bash
cp claude/commands/gptbrowser.md ~/.claude/commands/gptbrowser.md
```

Then:

```
/gptbrowser chatgpt.com what's the capital of France?
```

The command splits `$ARGUMENTS` into URL (first token) + prompt (the rest), runs exactly one `gptbrowser` call, and returns **only** its stdout. Its `allowed-tools: Bash(gptbrowser:*)` frontmatter scopes it so it can *only* ever shell out to `gptbrowser`.

**Option B — the MCP server** (any agent can reach it, not just someone typing the slash command):

```bash
claude mcp add gptbrowser -- node /abs/path/to/gpt-browser-automator/mcp/dist/index.js
```

If `gptbrowser` isn't on the PATH Claude Code launches from, pass its location:

```bash
claude mcp add gptbrowser --env GPTBROWSER_BIN=/opt/homebrew/bin/gptbrowser \
  -- node /abs/path/to/gpt-browser-automator/mcp/dist/index.js
```

**Option C — the sub-agent "feels like an API" pattern** (the reason this tool exists):

```js
Agent({
  description: "Ask Gemini for a second opinion",
  prompt: `Run exactly one Bash command: gptbrowser gemini.google.com "<question>".
           Return its stdout verbatim as your final message, no commentary.`
})
```

The sub-agent's *marginal* cost drops to one Bash call + one text blob back — ideal for parallel fan-out (launch several agents, each hitting a different site). See [Benchmarks](#benchmarks).

### Codex CLI

Codex consumes a stdio MCP server descriptor (`command` + `args` + optional `env`). Add to your Codex MCP config:

```toml
# ~/.codex/config.toml
[mcp_servers.gptbrowser]
command = "node"
args = ["/abs/path/to/gpt-browser-automator/mcp/dist/index.js"]
env = { GPTBROWSER_BIN = "/opt/homebrew/bin/gptbrowser" }
```

Or skip MCP entirely and let Codex call the binary directly in a shell step — it's just a program on `PATH`:

```bash
gptbrowser claude.ai "review this function for edge cases: $(cat foo.rs)"
```

### Antigravity

Antigravity is MCP-native. Register the same stdio server:

```json
{
  "mcpServers": {
    "gptbrowser": {
      "command": "node",
      "args": ["/abs/path/to/gpt-browser-automator/mcp/dist/index.js"],
      "env": { "GPTBROWSER_BIN": "/opt/homebrew/bin/gptbrowser" }
    }
  }
}
```

Then ask the agent to use the `ask_model` / `list_models` tools. Schemas are in [The MCP server](#the-mcp-server).

### Cursor / Windsurf / Zed / generic MCP client

Any client that accepts a `command` + `args` (+ optional `env`) stdio descriptor uses the identical shape — Cursor's `~/.cursor/mcp.json`, Windsurf's MCP config, Zed's `context_servers`, etc.:

```json
{
  "command": "node",
  "args": ["/abs/path/to/gpt-browser-automator/mcp/dist/index.js"],
  "env": { "GPTBROWSER_BIN": "/opt/homebrew/bin/gptbrowser" }
}
```

Adjust only the outer key the specific client expects; the `command`/`args`/`env` triple is portable.

### Plain shells & CI

Nothing is Claude- or MCP-specific. Any shell, script, Makefile, or CI step that can run a binary can use it directly:

```bash
gptbrowser --url kimi.com --prompt "summarize the attached diff"
git diff | gptbrowser deepseek.com "any obvious bugs in this diff?"
gptbrowser --json chatgpt.com "ping" | jq -r .response
```

The only prerequisites are a running PinchTab daemon and a populated `~/.pinchtab/config.json` (or `PINCHTAB_URL`/`PINCHTAB_TOKEN`) in the environment the process inherits.

---

## The MCP server

A stdio [MCP](https://modelcontextprotocol.io) server in [`mcp/`](mcp/) wraps the CLI so any MCP-capable tool gets typed tools instead of parsing stdout. Build it with `cd mcp && npm install && npm run build`.

### `ask_model`

Runs `gptbrowser --json [--model <model>] [--timeout <timeout_sec>] <url> <prompt>` and returns the `response` text. On non-zero exit, the result is `isError: true` with the parsed `gptbrowser: error: …` message.

| field | type | required | description |
|---|---|---|---|
| `url` | string | yes | e.g. `chatgpt.com`, `gemini.com`, `claude.ai`, … |
| `prompt` | string | yes | the prompt text |
| `model` | string | no | switch model before prompting |
| `timeout_sec` | number | no | per-call timeout, passed to `--timeout` |

### `list_models`

Runs `gptbrowser --list-models <url>` and returns the newline-separated model list.

| field | type | required | description |
|---|---|---|---|
| `url` | string | yes | the chatbot site to query |

**Notes:** arguments are passed via `execFile` with an argument array (never shell-interpolated), so prompts with quotes/newlines are safe. Override the binary path with `GPTBROWSER_BIN`. Full details in [`mcp/README.md`](mcp/README.md).

---

## Benchmarks

Measured with Sonnet general-purpose sub-agents on a 3-sentence prompt (full method + numbers in [BENCHMARKS.md](BENCHMARKS.md)):

| Arm | Tokens | Tool calls | Wall time |
|---|---|---|---|
| **gptbrowser** (1 call) | ≈ 51.3 k | **1** | ~21 s |
| **Direct PinchTab** (given exact selectors) | ≈ 55.7–59 k | **7–11** | 61–199 s |

Absolute totals are close because a fixed ~51 k system-prompt floor dominates both — but the **marginal task cost** is ~near-zero (tool) vs ~5 k (direct), i.e. **10–50×**, and that's *conservative*: the direct agents were handed selectors, skipping the DOM-discovery cost the tool encodes once. The gap widens with response length, since direct polling re-reads the growing answer into context while `gptbrowser` polls only an integer length.

---

## Knowledge graph

[`graph/`](graph/) holds a [graphify](https://pypi.org/project/graphifyy/) knowledge graph of the source — **110 nodes · 252 edges · 12 communities** (open [`graph/graph.html`](graph/graph.html) in a browser). Regenerate after code changes with the LLM-free extractor:

```bash
graphify update .
cp graphify-out/{graph.json,graph.html,GRAPH_REPORT.md} graph/
```

---

## Project layout

```
gpt-browser-automator/
├── src/main.rs            # the entire CLI (~1050 lines): Site enum + per-site adapters,
│                          #   injection strategies, completion detector, model switching,
│                          #   image download, PinchTab HTTP client
├── build.sh              # cargo build --release + install to /opt/homebrew/bin
├── Cargo.toml            # deps: ureq (no-TLS) + serde_json; release LTO + strip
├── claude/commands/      # the /gptbrowser slash command (mirror of ~/.claude/commands)
├── mcp/                  # stdio MCP server (TypeScript): ask_model / list_models
├── docs/USAGE.md         # per-tool usage deep-dive
├── graph/                # graphify knowledge graph (html + json + report)
├── BENCHMARKS.md         # token / tool-call / latency comparison
└── README.md
```

---

## Troubleshooting

| Error | Meaning & fix |
|---|---|
| `composer never appeared on <site> — are you logged in?` | The composer selector didn't show up within 30 s. Open the site manually in the PinchTab Chrome and confirm you're signed in. Copilot/Grok sit behind a wall that blocks unattended use. |
| `request failed: … (is the PinchTab daemon running?)` | The HTTP call to the daemon failed. Start/restart PinchTab; confirm `~/.pinchtab/config.json` has a valid `server.token`/port, or set `PINCHTAB_URL`/`PINCHTAB_TOKEN`. |
| `prompt did not land in <site> composer (got N chars, expected ~M)` | Injection landed partially — usually the site changed its composer DOM. Compare the live DOM to that site's `composer_sel()` / `inject_js()` arm in `src/main.rs`. |
| `<site> produced no response within 45s of sending` | Send didn't register, or the site is slow to start. This is bounded by the fixed 45 s first-token wait, *not* `--timeout`. |
| `warning — hit <N>s timeout; returning partial response` (stderr) | Generation exceeded `--timeout`; partial text is returned. Raise `--timeout`, or if it trips every call, bump `--stable`. |
| Model switch fails: "not found" / "not selectable" | The error lists the models the picker actually offered; free/guest tiers gate the choice. |
| `command not found` from an agent but not your shell | The binary isn't on the agent's `PATH`. Reinstall to `/opt/homebrew/bin` (default) or point `GPTBROWSER_BIN` at it for MCP. |

---

## Building from source

```bash
cargo build --release          # target/release/gptbrowser
cargo run --release -- --help   # run without installing
./build.sh                      # build + install to $PREFIX (default /opt/homebrew/bin)
```

The release profile enables LTO and symbol stripping (see `Cargo.toml`) for a small, fast binary. There is no `unsafe`, no async runtime, and no TLS stack — `ureq` talks plain HTTP to `localhost`.

---

## Security notes

- **Runs entirely against `localhost`.** `gptbrowser` only talks to your local PinchTab daemon; it opens no outbound connections of its own.
- **Uses your real session.** It acts as *you* in your logged-in Chrome. Treat prompts and responses with the same care as anything you'd type into those sites yourself.
- **Token handling.** The PinchTab Bearer token is read from `~/.pinchtab/config.json` or env vars and sent only to the local daemon. Keep that file readable only by you (`chmod 600`).
- **MCP arg safety.** The MCP server invokes the binary via `execFile` with an argument array — prompts are never passed through a shell, so quotes/newlines/metacharacters can't inject commands.

---

## Limitations & roadmap

- **Copilot & Grok** are login-gated; their adapters exist but need an authenticated profile.
- **Model switching** is not available on ChatGPT / Perplexity / z.ai (tier-gated in the UI).
- **Image generation** is ChatGPT-only today and subject to that account's daily cap. A per-site `image_src_js` could extend it to Gemini (googleusercontent srcs) — a natural next step.
- **Selector drift.** Sites change their DOM; adapters are pinned to CSS selectors in `src/main.rs` and may need occasional updates. Each failure mode prints a targeted, self-diagnosing error to make that quick.

---

## Contributing

Issues and PRs welcome. High-value contributions:

- **New site adapters** — implement the `Site` enum methods (`composer_present_js`, `composer_sel`, `inject_js`, submit, `response_js`, `generating_js`).
- **Selector fixes** when a site changes its DOM.
- **Model-switch coverage** for sites that expose a picker.

Keep the design constraints intact: one small binary, no heavy deps, no async runtime, adapters as data on the `Site` enum. Run `cargo build --release` and a quick live smoke test against at least one working site before opening a PR.

---

## License

Released under the [MIT License](LICENSE).

---

*Built to make chatbot web UIs callable like an API — cheaply, deterministically, and from any tool on your machine.*

# Usage

How to reach `gptbrowser` from each coding tool. Pick the section for your setup — the
underlying mechanism (PinchTab + the `gptbrowser` binary) is the same everywhere; only
the calling convention changes.

## Claude Code

### The `/gptbrowser` slash command

A copy of the shipped command lives in this repo at
[`claude/commands/gptbrowser.md`](../claude/commands/gptbrowser.md), mirroring
`~/.claude/commands/gptbrowser.md`. Install it by symlinking or copying it into your
Claude Code commands directory:

```bash
cp claude/commands/gptbrowser.md ~/.claude/commands/gptbrowser.md
```

Once installed, invoke it as:

```
/gptbrowser chatgpt.com what's the capital of France?
```

The command's contract (see the file for the exact prompt Claude Code runs):

1. Splits `$ARGUMENTS` into URL (first whitespace-separated token) and PROMPT (the
   rest).
2. Runs exactly one Bash call: `gptbrowser --url <URL> --prompt "<PROMPT>"`.
3. Returns **only** the command's stdout — no preamble, no summary. The stdout *is*
   the reply.
4. On a non-zero exit, reports the `gptbrowser: error: ...` line from stderr verbatim
   and stops.

It also tells the calling agent explicitly not to open a browser itself or use any
other browser tool — `gptbrowser` already owns that.

> **Known gap**: the shipped command's description and inline docs still say
> "ChatGPT/Gemini/Kimi/DeepSeek" and list only those four site names in its
> instructions. The underlying binary (`src/main.rs`) actually supports nine sites —
> Claude, Perplexity, and z.ai are working today, and Copilot/Grok have adapters
> pending login. If you're maintaining the command text, update it to match
> `main.rs`'s `Site` enum rather than trusting the command file's own site list.

The `allowed-tools: Bash(gptbrowser:*)` frontmatter scopes the command to only ever
shell out to `gptbrowser` — it can't be used to smuggle in arbitrary Bash.

### The MCP server

For a tool-call interface instead of a slash command (e.g. so any agent, not just one
typing `/gptbrowser`, can reach for it), point Claude Code at the MCP server in
`mcp/` — it exposes `ask_model` and `list_models` as MCP tools over the same
PinchTab-driven flow. See `mcp/README.md` for the server's own setup and tool schemas.

### The sub-agent "feels like an API" pattern

The pattern this whole tool exists to support: spin up a sub-agent (via the `Agent`
tool) and hand it a `gptbrowser` call as its entire task, rather than letting it drive
a browser itself.

```
Agent({
  description: "Ask Gemini for a second opinion",
  prompt: "Run exactly one Bash command: gptbrowser gemini.google.com \"<question>\".
           Return its stdout verbatim as your final message, no commentary."
})
```

The sub-agent pays a fixed system-prompt token floor either way, but its *marginal*
cost drops to roughly the size of one Bash call and one text blob back — see
[BENCHMARKS.md](../BENCHMARKS.md) for the measured difference against driving
PinchTab directly. This is also why `gptbrowser` is a good fit for parallel fan-out:
launch several sub-agents, each shelling out to `gptbrowser` against a different site,
and each one comes back having spent only trivial marginal tokens on the round trip.

## Codex / other shells (no MCP, no slash commands)

There's nothing Claude-specific about the binary — call it directly:

```bash
gptbrowser claude.ai "review this function for edge cases: $(cat foo.rs)"
gptbrowser --url kimi.com --prompt "summarize the attached diff" 
git diff | gptbrowser deepseek.com "any obvious bugs in this diff?"
```

`--json` gives a stable, parseable shape for scripting:

```bash
gptbrowser --json chatgpt.com "ping" | jq -r .response
```

```json
{
  "model": "chatgpt",
  "url": "https://chatgpt.com/",
  "prompt": "ping",
  "response": "pong! how can I help?",
  "elapsed_ms": 8421
}
```

Exit code is non-zero on failure, with a `gptbrowser: error: ...` line on stderr —
safe to check in a shell script the normal way:

```bash
if ! out=$(gptbrowser claude.ai "$PROMPT" 2>err.log); then
  echo "gptbrowser failed: $(cat err.log)" >&2
  exit 1
fi
```

Any coding agent/shell that can run an arbitrary binary (Codex CLI, plain shell
scripts, CI steps) uses this exact form — no Claude Code, no MCP, no slash command
required. The only prerequisite is that the PinchTab daemon is running and
`~/.pinchtab/config.json` is populated (or `PINCHTAB_URL`/`PINCHTAB_TOKEN` are set in
the environment the agent runs in).

## Antigravity / any MCP client

For MCP-native tools (Antigravity, or any other MCP client) that would rather call a
typed tool than shell out to a binary, use the MCP server in `mcp/` instead of the raw
CLI. It wraps the same PinchTab-driven flow as MCP tools (`ask_model`, `list_models`)
with proper JSON schemas, so the client gets structured input/output without needing
to shell out or parse stdout.

Configuration and tool schemas: see `mcp/README.md`. This repo does not duplicate that
setup here — treat `mcp/` as the source of truth for anything MCP-transport-specific.

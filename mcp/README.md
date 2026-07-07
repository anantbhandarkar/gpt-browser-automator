# gptbrowser-mcp

A stdio [MCP](https://modelcontextprotocol.io) server that wraps the `gptbrowser` CLI so any
MCP-capable coding tool (Claude Code, Codex, Antigravity, etc.) can prompt logged-in chatbot
web UIs — ChatGPT, Gemini, Kimi, DeepSeek, Claude.ai, Perplexity, chat.z.ai, and more — through
the user's already-open, logged-in Chrome (driven by PinchTab under the hood).

## Prerequisites

- Node.js >= 18
- The `gptbrowser` CLI installed and on `$PATH` (or set `GPTBROWSER_BIN` to its absolute path).
- Chrome open and logged in to whichever chatbot site(s) you want to query.

## Build

```bash
cd mcp
npm install
npm run build
```

This produces `dist/index.js`, the compiled stdio server entrypoint.

## Tools exposed

### `ask_model`

Runs `gptbrowser --json [--model <model>] [--timeout <timeout_sec>] <url> <prompt>` and returns
the chatbot's `response` text as the tool result. On a non-zero exit from `gptbrowser`, the tool
result has `isError: true` and the content is the parsed `gptbrowser: error: ...` message (or raw
stderr if it doesn't match that shape).

Inputs:

| field         | type   | required | description                                              |
|---------------|--------|----------|----------------------------------------------------------|
| `url`         | string | yes      | e.g. `chatgpt.com`, `gemini.com`, `kimi.com`, `chat.deepseek.com`, `claude.ai`, `perplexity.ai`, `chat.z.ai` |
| `prompt`      | string | yes      | the prompt text to send                                  |
| `model`       | string | no       | switch model before prompting                             |
| `timeout_sec` | number | no       | per-call timeout passed through to `gptbrowser --timeout` |

### `list_models`

Runs `gptbrowser --list-models <url>` and returns the newline-separated list of available models
for that site.

Inputs:

| field | type   | required | description        |
|-------|--------|----------|---------------------|
| `url` | string | yes      | the chatbot site to query |

## Registering with Claude Code

```bash
claude mcp add gptbrowser -- node /abs/path/to/gpt-browser-automator/mcp/dist/index.js
```

Or add directly to a project's `.mcp.json`:

```json
{
  "mcpServers": {
    "gptbrowser": {
      "command": "node",
      "args": ["/abs/path/to/gpt-browser-automator/mcp/dist/index.js"]
    }
  }
}
```

If `gptbrowser` isn't on `$PATH` for the environment Claude Code launches from, pass it via `env`:

```json
{
  "mcpServers": {
    "gptbrowser": {
      "command": "node",
      "args": ["/abs/path/to/gpt-browser-automator/mcp/dist/index.js"],
      "env": {
        "GPTBROWSER_BIN": "/opt/homebrew/bin/gptbrowser"
      }
    }
  }
}
```

## Registering with a generic MCP client / Codex / Antigravity

Any MCP client that accepts a stdio server command/args pair can use the same shape. Example
generic config (adjust the key names/format to whatever the specific client expects — Codex and
Antigravity both consume a `command` + `args` (+ optional `env`) stdio server descriptor):

```json
{
  "command": "node",
  "args": ["/abs/path/to/gpt-browser-automator/mcp/dist/index.js"],
  "env": {
    "GPTBROWSER_BIN": "/opt/homebrew/bin/gptbrowser"
  }
}
```

## Example tool call

Once registered, ask the agent to use the tool, e.g.:

> Use gptbrowser's `ask_model` tool to ask gemini.com: "What's the capital of France?"

which results in a call like:

```json
{
  "name": "ask_model",
  "arguments": {
    "url": "gemini.com",
    "prompt": "What's the capital of France?"
  }
}
```

returning the model's plain-text reply as the tool result.

## Notes

- Arguments are passed to `gptbrowser` via `execFile` with an argument array — never shell string
  interpolation — so prompts containing quotes, newlines, or other special characters are safe.
- The exec timeout defaults to 300s regardless of `timeout_sec`, giving `gptbrowser` (which itself
  can take 10-60s) plenty of headroom; if `timeout_sec` is set the exec timeout is extended to
  `timeout_sec + 30s` when that's larger.
- Override the CLI binary location with the `GPTBROWSER_BIN` environment variable (defaults to
  `gptbrowser`, resolved via `$PATH`).

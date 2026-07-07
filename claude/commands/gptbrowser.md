---
description: Prompt a logged-in chatbot web UI (ChatGPT/Gemini/Claude/Kimi/DeepSeek/Perplexity/z.ai) via PinchTab and return the clean, HTML-free text response.
argument-hint: <url> [--model <name>] <prompt>
allowed-tools: Bash(gptbrowser:*)
---

Get a deterministic, HTML-free response from a chatbot web UI using the `gptbrowser` CLI.

Raw arguments: `$ARGUMENTS`

The FIRST whitespace-separated token is the site URL — one of:
`chatgpt.com`, `gemini.com`, `claude.ai`, `kimi.com`, `chat.deepseek.com`, `perplexity.ai`, `chat.z.ai`
(also `copilot.microsoft.com` / `grok.com`, which require you to be signed in). Everything after the URL is the prompt.

Do this:
1. Split `$ARGUMENTS`: URL = first token, PROMPT = the remainder. If the caller included `--model <name>`, pass it through.
2. Run one Bash command: `gptbrowser --url <URL> [--model <name>] --prompt "<PROMPT>"` — single-quote/escape the prompt so shell metacharacters are preserved literally.
3. Return **only** the command's stdout as the answer — no preamble, no commentary. The stdout IS the reply, verbatim.
4. If the command exits non-zero, report the `gptbrowser: error: ...` line from stderr verbatim and stop.

Notes:
- `gptbrowser` drives the already-open, logged-in Chrome via PinchTab. Do not open the browser yourself or use any other browser tool.
- Model switching (`--model`, fuzzy match) works on gemini / kimi / claude / deepseek (DeepThink/Search toggle). Run `gptbrowser --list-models <url>` to see a site's options. chatgpt / perplexity / z.ai gate model choice behind a paid tier or login.
- Add `--json` for structured `{model,url,prompt,response,elapsed_ms}` instead of bare text.

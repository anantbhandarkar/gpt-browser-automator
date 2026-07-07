---
description: Prompt a logged-in chatbot web UI (ChatGPT/Gemini/Kimi/DeepSeek) via PinchTab and return the clean, HTML-free text response.
argument-hint: <url> <prompt>
allowed-tools: Bash(gptbrowser:*)
---

Get a deterministic, HTML-free response from a chatbot web UI using the `gptbrowser` CLI.

Raw arguments: `$ARGUMENTS`

The FIRST whitespace-separated token is the site URL — one of `chatgpt.com`, `gemini.com`, `kimi.com`, `chat.deepseek.com`. Everything after it is the prompt.

Do this:
1. Split `$ARGUMENTS`: URL = first token, PROMPT = the remainder.
2. Run exactly one Bash command: `gptbrowser --url <URL> --prompt "<PROMPT>"` — single-quote or escape the prompt so shell metacharacters are preserved literally.
3. Return **only** the command's stdout as the answer. No preamble, no commentary, no summary — the stdout IS the reply, verbatim.
4. If the command exits non-zero, report the `gptbrowser: error: ...` line from stderr verbatim and stop.

Notes:
- `gptbrowser` drives the already-open, logged-in Chrome via PinchTab. Do not open the browser yourself, do not use any other browser tool.
- It reuses an existing tab for the site (or opens one), injects the prompt, waits for generation to finish deterministically, and prints the clean text.
- Add `--json` if the caller wants structured `{model,url,prompt,response,elapsed_ms}` instead of bare text.

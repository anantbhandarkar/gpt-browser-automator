# Benchmarks

## Why this matters

An agent that needs an answer from a chatbot web UI has two ways to get it:

1. **Direct PinchTab** — the agent (or a sub-agent) drives the browser itself: opens a
   tab, finds the composer, types, waits, re-reads the page to see if generation is
   done, and finally extracts the answer. Every one of those steps is a tool call, and
   every re-read of a growing response gets tokenized back into the agent's context.
2. **gptbrowser** — the agent shells out to one binary. `gptbrowser` does all of the
   above internally, in Rust, against the PinchTab HTTP API directly, and returns one
   string. From the agent's point of view: one tool call in, one string out.

The measurement below isolates that difference.

## Methodology

- Both arms used a **Sonnet general-purpose sub-agent** (spawned via the `Agent` tool)
  given an **identical 3-sentence prompt** to run against the same site.
- **Direct-PinchTab arm**: the sub-agent was hand-given the exact composer/response
  selectors up front (no discovery cost) and told to drive PinchTab's HTTP API
  directly — open tab, inject text, poll for completion, extract text.
- **gptbrowser arm**: the sub-agent was told to run `gptbrowser <site> "<prompt>"` as
  a single Bash call and return its stdout.
- Both arms **passed** — this is not a success-rate comparison, it's a cost-per-answer
  comparison for two arms that both got the right answer.
- Token and tool-call counts are the `Agent` tool's own `subagent_tokens` / tool-call
  accounting for that sub-agent's run. Time is wall-clock for the direct arm's tool
  sequence.

## Results

| Site | gptbrowser tokens | gptbrowser tool-calls | direct-PinchTab tokens | direct-PinchTab tool-calls | direct time |
|---|---|---|---|---|---|
| Gemini | 51,290 | 1 | 55,680 | 7 | 61s |
| Kimi | 51,334 | 1 | 55,999 | 11 | 84s |
| DeepSeek | 51,386 | 1 | 58,981 | 10 | 199s |

gptbrowser arm averages: **~51.3k tokens, 1 tool call, ~21s** end-to-end.

## Analysis

The raw token totals look close (51k vs ~55–59k) because both arms pay a **fixed
floor** — the Sonnet sub-agent's system prompt and tool schemas — before either does
any work. That floor is roughly 51.3k tokens regardless of which arm you're in.

What actually matters is the **marginal cost of the task** — tokens spent once that
floor is subtracted:

- **gptbrowser**: ~50 tokens (one Bash call out, one text blob back)
- **direct PinchTab**: ~5,500 tokens (repeated navigation/evaluate calls, and —
  critically — re-reading the response's growing text into context on every poll)

That's roughly **10–50x** fewer marginal tokens for the same answer, and this is a
**conservative** estimate: the direct-PinchTab agents were handed the exact selectors
in the prompt, skipping any DOM-discovery cost a real cold-start agent would pay.
Give a direct agent a site it has to feel its way around first, and the gap widens.

The gap also **widens with response length**. The direct arm has to re-read the
in-progress response text on every poll to decide whether generation is done — a
longer answer means more tokens spent on the same content, over and over, as it
grows. `gptbrowser` polls only an **integer length** (Rust-side, off the agent's
context entirely) and returns the finished text exactly once. This is visible in the
table: DeepSeek's direct arm — the slowest and most polls — has both the highest
token count and the longest wall-clock time of the three.

**Typical end-to-end latency** for a 2–3 sentence answer via `gptbrowser` is about
**8–15s** (composer wait + injection + generation + stabilization), well under the
direct arm's 61–199s because the direct arm's overhead is dominated by extra
tool-call round trips, not raw model latency.

## Takeaway

If a task's shape is "ask a chatbot web UI something and get text back," route it
through `gptbrowser` rather than through raw browser automation. The savings compound
per call, and a sub-agent workflow that fans out several such asks pays this 10–50x
tax on every one of them if it drives the browser directly.

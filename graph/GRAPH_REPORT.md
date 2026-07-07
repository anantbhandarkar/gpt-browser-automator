# Graph Report - gpt-browser-automator  (2026-07-07)

## Corpus Check
- 2 files · ~18,454 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 110 nodes · 252 edges · 12 communities detected
- Extraction: 99% EXTRACTED · 1% INFERRED · 0% AMBIGUOUS · INFERRED: 3 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- [[_COMMUNITY_Community 0|Community 0]]
- [[_COMMUNITY_Community 1|Community 1]]
- [[_COMMUNITY_Community 2|Community 2]]
- [[_COMMUNITY_Community 3|Community 3]]
- [[_COMMUNITY_Community 4|Community 4]]
- [[_COMMUNITY_Community 5|Community 5]]
- [[_COMMUNITY_Community 6|Community 6]]
- [[_COMMUNITY_Community 7|Community 7]]
- [[_COMMUNITY_Community 9|Community 9]]
- [[_COMMUNITY_Community 10|Community 10]]
- [[_COMMUNITY_Community 11|Community 11]]
- [[_COMMUNITY_Community 12|Community 12]]

## God Nodes (most connected - your core abstractions)
1. `run()` - 28 edges
2. `gptbrowser CLI` - 22 edges
3. `Site` - 18 edges
4. `open_picker()` - 12 edges
5. `switch_model()` - 12 edges
6. `gptbrowser-mcp stdio server` - 9 edges
7. `Client` - 8 edges
8. `click_marked()` - 8 edges
9. `list_models()` - 8 edges
10. `Known gap: command site-list drift` - 8 edges

## Surprising Connections (you probably didn't know these)
- `gptbrowser CLI` --uses--> `Slash command contract`  [EXTRACTED]
  README.md → docs/USAGE.md
- `gptbrowser CLI` --uses--> `Codex / other shells usage section`  [EXTRACTED]
  README.md → docs/USAGE.md
- `--model flag` --references--> `claude/commands/gptbrowser.md command file`  [EXTRACTED]
  README.md → claude/commands/gptbrowser.md
- `Tab-foreground trick` --conceptually_related_to--> `Marginal cost analysis`  [INFERRED]
  README.md → BENCHMARKS.md
- `Benchmarks takeaway` --conceptually_related_to--> `Sub-agent "feels like an API" pattern`  [INFERRED]
  BENCHMARKS.md → docs/USAGE.md

## Hyperedges (group relationships)
- **** —  [INFERRED 0.70]
- **** —  [INFERRED 0.65]
- **** —  [INFERRED 0.60]

## Communities

### Community 0 - "Community 0"
Cohesion: 0.17
Nodes (7): acquire_tab(), as_text(), await_image(), await_response(), run(), send(), Site

### Community 1 - "Community 1"
Cohesion: 0.26
Nodes (15): click_marked(), close_picker(), list_models(), mark_option(), mark_selector(), mark_text(), normalize_url(), open_picker() (+7 more)

### Community 2 - "Community 2"
Cohesion: 0.23
Nodes (9): Args, base64_decode(), Client, download_image(), load_config(), main(), parse(), parse_args() (+1 more)

### Community 3 - "Community 3"
Cohesion: 0.2
Nodes (11): ask_model tool, MCP exec timeout defaults, execFile argument-array safety, GPTBROWSER_BIN env var, gptbrowser-mcp stdio server, list_models tool, --list-models flag, gptbrowser MCP server (+3 more)

### Community 4 - "Community 4"
Cohesion: 0.32
Nodes (8): Completion detection step, Composer wait step, Extract and return step, Prompt injection techniques, Submit step, Tab acquisition step, Tab-foreground trick, Troubleshooting guide

### Community 5 - "Community 5"
Cohesion: 0.48
Nodes (7): claude/commands/gptbrowser.md command file, Copilot adapter, Grok adapter, Native textarea injection technique, Site::detect, z.ai adapter, Known gap: command site-list drift

### Community 6 - "Community 6"
Cohesion: 0.48
Nodes (7): gptbrowser vs direct-PinchTab token benchmark, build.sh, gptbrowser CLI, PinchTab, PinchTab HTTP client (ureq-based), Claude Code usage section, Sub-agent "feels like an API" pattern

### Community 7 - "Community 7"
Cohesion: 0.4
Nodes (5): Marginal cost analysis, Benchmark methodology, Benchmark results (Gemini/Kimi/DeepSeek), Benchmarks takeaway, DeepSeek adapter

### Community 9 - "Community 9"
Cohesion: 0.5
Nodes (4): ChatGPT adapter, Claude adapter, Gemini adapter, ProseMirror/Quill/tiptap injection technique

### Community 10 - "Community 10"
Cohesion: 0.67
Nodes (3): allowed-tools: Bash(gptbrowser:*) scoping, Slash command contract, /gptbrowser slash command

### Community 11 - "Community 11"
Cohesion: 0.67
Nodes (3): Kimi adapter, Lexical injection technique, Perplexity adapter

### Community 12 - "Community 12"
Cohesion: 1.0
Nodes (2): Codex / other shells usage section, --json flag

## Knowledge Gaps
- **7 isolated node(s):** `Extract and return step`, `build.sh`, `allowed-tools: Bash(gptbrowser:*) scoping`, `Antigravity / any MCP client usage section`, `GPTBROWSER_BIN env var` (+2 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **Thin community `Community 12`** (2 nodes): `Codex / other shells usage section`, `--json flag`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `gptbrowser CLI` connect `Community 6` to `Community 3`, `Community 5`, `Community 7`, `Community 9`, `Community 10`, `Community 11`, `Community 12`?**
  _High betweenness centrality (0.118) - this node is a cross-community bridge._
- **Why does `run()` connect `Community 0` to `Community 1`, `Community 2`?**
  _High betweenness centrality (0.067) - this node is a cross-community bridge._
- **Why does `Site` connect `Community 0` to `Community 1`?**
  _High betweenness centrality (0.056) - this node is a cross-community bridge._
- **What connects `Extract and return step`, `build.sh`, `allowed-tools: Bash(gptbrowser:*) scoping` to the rest of the system?**
  _7 weakly-connected nodes found - possible documentation gaps or missing edges._
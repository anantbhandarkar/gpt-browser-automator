# Graph Report - gpt-browser-automator  (2026-07-07)

## Corpus Check
- Corpus is ~7,718 words - fits in a single context window. You may not need a graph.

## Summary
- 102 nodes · 227 edges · 12 communities detected
- Extraction: 99% EXTRACTED · 1% INFERRED · 0% AMBIGUOUS · INFERRED: 3 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- [[_COMMUNITY_CLI entry & args|CLI entry & args]]
- [[_COMMUNITY_MCP server|MCP server]]
- [[_COMMUNITY_Model-switch engine|Model-switch engine]]
- [[_COMMUNITY_PinchTab HTTP client|PinchTab HTTP client]]
- [[_COMMUNITY_Site adapters|Site adapters]]
- [[_COMMUNITY_Token benchmarks|Token benchmarks]]
- [[_COMMUNITY_Synthetic click helpers|Synthetic click helpers]]
- [[_COMMUNITY_Adapters & injection|Adapters & injection]]
- [[_COMMUNITY_Sendresponse pipeline|Send/response pipeline]]
- [[_COMMUNITY_PinchTab bridge|PinchTab bridge]]
- [[_COMMUNITY_Claude slash command|Claude slash command]]
- [[_COMMUNITY_Misc|Misc]]

## God Nodes (most connected - your core abstractions)
1. `run()` - 24 edges
2. `gptbrowser CLI` - 22 edges
3. `Site` - 16 edges
4. `open_picker()` - 12 edges
5. `switch_model()` - 12 edges
6. `gptbrowser-mcp stdio server` - 9 edges
7. `click_marked()` - 8 edges
8. `list_models()` - 8 edges
9. `Known gap: command site-list drift` - 8 edges
10. `claude/commands/gptbrowser.md command file` - 8 edges

## Surprising Connections (you probably didn't know these)
- `Slash command contract` --uses--> `gptbrowser CLI`  [EXTRACTED]
  docs/USAGE.md → README.md
- `Codex / other shells usage section` --uses--> `gptbrowser CLI`  [EXTRACTED]
  docs/USAGE.md → README.md
- `claude/commands/gptbrowser.md command file` --references--> `--model flag`  [EXTRACTED]
  claude/commands/gptbrowser.md → README.md
- `Marginal cost analysis` --conceptually_related_to--> `Tab-foreground trick`  [INFERRED]
  BENCHMARKS.md → README.md
- `claude/commands/gptbrowser.md command file` --references--> `--json flag`  [EXTRACTED]
  claude/commands/gptbrowser.md → docs/USAGE.md

## Hyperedges (group relationships)
- **** —  [INFERRED 0.70]
- **** —  [INFERRED 0.65]
- **** —  [INFERRED 0.60]

## Communities

### Community 0 - "CLI entry & args"
Cohesion: 0.15
Nodes (8): acquire_tab(), Args, main(), normalize_url(), parse_args(), run(), send(), Site

### Community 1 - "MCP server"
Cohesion: 0.2
Nodes (11): ask_model tool, MCP exec timeout defaults, execFile argument-array safety, GPTBROWSER_BIN env var, gptbrowser-mcp stdio server, list_models tool, --list-models flag, gptbrowser MCP server (+3 more)

### Community 2 - "Model-switch engine"
Cohesion: 0.4
Nodes (7): close_picker(), list_models(), mark_option(), mark_text(), options_count(), read_options(), switch_model()

### Community 3 - "PinchTab HTTP client"
Cohesion: 0.31
Nodes (5): as_text(), await_response(), Client, load_config(), parse()

### Community 4 - "Site adapters"
Cohesion: 0.31
Nodes (9): build.sh, ChatGPT adapter, Claude adapter, Gemini adapter, gptbrowser CLI, Kimi adapter, Lexical injection technique, Perplexity adapter (+1 more)

### Community 5 - "Token benchmarks"
Cohesion: 0.29
Nodes (8): Marginal cost analysis, Benchmark methodology, Benchmark results (Gemini/Kimi/DeepSeek), Benchmarks takeaway, gptbrowser vs direct-PinchTab token benchmark, DeepSeek adapter, Claude Code usage section, Sub-agent "feels like an API" pattern

### Community 6 - "Synthetic click helpers"
Cohesion: 0.62
Nodes (6): click_marked(), mark_selector(), open_picker(), synth_click_marked(), unmark(), wait_until()

### Community 7 - "Adapters & injection"
Cohesion: 0.48
Nodes (7): claude/commands/gptbrowser.md command file, Copilot adapter, Grok adapter, Native textarea injection technique, Site::detect, z.ai adapter, Known gap: command site-list drift

### Community 8 - "Send/response pipeline"
Cohesion: 0.38
Nodes (7): Completion detection step, Composer wait step, Extract and return step, Prompt injection techniques, Submit step, Tab-foreground trick, Troubleshooting guide

### Community 10 - "PinchTab bridge"
Cohesion: 0.67
Nodes (3): PinchTab, PinchTab HTTP client (ureq-based), Tab acquisition step

### Community 11 - "Claude slash command"
Cohesion: 0.67
Nodes (3): allowed-tools: Bash(gptbrowser:*) scoping, Slash command contract, /gptbrowser slash command

### Community 12 - "Misc"
Cohesion: 1.0
Nodes (2): Codex / other shells usage section, --json flag

## Knowledge Gaps
- **7 isolated node(s):** `Extract and return step`, `build.sh`, `allowed-tools: Bash(gptbrowser:*) scoping`, `Antigravity / any MCP client usage section`, `GPTBROWSER_BIN env var` (+2 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **Thin community `Misc`** (2 nodes): `Codex / other shells usage section`, `--json flag`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `gptbrowser CLI` connect `Site adapters` to `MCP server`, `Token benchmarks`, `Adapters & injection`, `PinchTab bridge`, `Claude slash command`, `Misc`?**
  _High betweenness centrality (0.138) - this node is a cross-community bridge._
- **Why does `run()` connect `CLI entry & args` to `Model-switch engine`, `PinchTab HTTP client`, `Synthetic click helpers`?**
  _High betweenness centrality (0.057) - this node is a cross-community bridge._
- **Why does `Site` connect `CLI entry & args` to `Model-switch engine`, `Synthetic click helpers`?**
  _High betweenness centrality (0.050) - this node is a cross-community bridge._
- **What connects `Extract and return step`, `build.sh`, `allowed-tools: Bash(gptbrowser:*) scoping` to the rest of the system?**
  _7 weakly-connected nodes found - possible documentation gaps or missing edges._
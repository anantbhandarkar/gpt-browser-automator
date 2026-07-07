#!/usr/bin/env node
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { execFile } from "node:child_process";
import { z } from "zod";

const GPTBROWSER_BIN = process.env.GPTBROWSER_BIN || "gptbrowser";
const EXEC_TIMEOUT_MS = 300_000; // 300s generous ceiling; gptbrowser itself is usually 10-60s
const MAX_BUFFER_BYTES = 20 * 1024 * 1024; // 20MB, plenty for text replies

interface ExecResult {
  stdout: string;
  stderr: string;
  code: number;
}

/**
 * Run the gptbrowser binary with an argument array (never shell string
 * interpolation) so prompts containing quotes/newlines/special chars are
 * always safe.
 */
function runGptBrowser(args: string[], timeoutMs: number = EXEC_TIMEOUT_MS): Promise<ExecResult> {
  return new Promise((resolve) => {
    execFile(
      GPTBROWSER_BIN,
      args,
      { timeout: timeoutMs, maxBuffer: MAX_BUFFER_BYTES },
      (error, stdout, stderr) => {
        if (error) {
          const code = typeof error.code === "number" ? error.code : 1;
          resolve({ stdout: stdout ?? "", stderr: stderr ?? error.message ?? "", code });
          return;
        }
        resolve({ stdout: stdout ?? "", stderr: stderr ?? "", code: 0 });
      }
    );
  });
}

function extractErrorMessage(stderr: string, fallback: string): string {
  const trimmed = stderr.trim();
  if (!trimmed) return fallback;
  const match = trimmed.match(/gptbrowser:\s*error:\s*(.+)/i);
  if (match) return match[1].trim();
  return trimmed;
}

const server = new McpServer({
  name: "gptbrowser-mcp",
  version: "1.0.0",
});

server.registerTool(
  "ask_model",
  {
    title: "Ask a chatbot web UI",
    description:
      "Prompt a logged-in chatbot web UI (ChatGPT, Gemini, Kimi, DeepSeek, Claude.ai, Perplexity, chat.z.ai, etc.) " +
      "via the user's already-open, logged-in Chrome and return the clean text reply. " +
      "Calls can take 10-60 seconds. Supported url args: chatgpt.com, gemini.com, kimi.com, " +
      "chat.deepseek.com, claude.ai, perplexity.ai, chat.z.ai (also copilot.microsoft.com / grok.com but those need login).",
    inputSchema: {
      url: z
        .string()
        .describe(
          "The chatbot site to target, e.g. chatgpt.com, gemini.com, kimi.com, chat.deepseek.com, claude.ai, perplexity.ai, chat.z.ai"
        ),
      prompt: z.string().describe("The prompt text to send to the chatbot."),
      model: z
        .string()
        .optional()
        .describe("Optional model name to switch to first (see list_models for options)."),
      timeout_sec: z
        .number()
        .optional()
        .describe("Optional per-call timeout in seconds passed through to gptbrowser --timeout."),
    },
  },
  async ({ url, prompt, model, timeout_sec }) => {
    const args: string[] = ["--json"];
    if (model) {
      args.push("--model", model);
    }
    if (typeof timeout_sec === "number") {
      args.push("--timeout", String(timeout_sec));
    }
    args.push(url, prompt);

    // Give the child process a bit more room than the requested timeout,
    // but always respect our own generous ceiling.
    const execTimeout =
      typeof timeout_sec === "number"
        ? Math.max(EXEC_TIMEOUT_MS, timeout_sec * 1000 + 30_000)
        : EXEC_TIMEOUT_MS;

    const result = await runGptBrowser(args, execTimeout);

    if (result.code !== 0) {
      const message = extractErrorMessage(
        result.stderr,
        `gptbrowser exited with code ${result.code}`
      );
      return {
        isError: true,
        content: [{ type: "text", text: message }],
      };
    }

    const raw = result.stdout.trim();
    try {
      const parsed = JSON.parse(raw) as {
        model?: string;
        url?: string;
        prompt?: string;
        response?: string;
        elapsed_ms?: number;
      };
      if (typeof parsed.response !== "string") {
        return {
          isError: true,
          content: [
            {
              type: "text",
              text: `gptbrowser --json output did not contain a "response" field: ${raw}`,
            },
          ],
        };
      }
      return {
        content: [{ type: "text", text: parsed.response }],
      };
    } catch (e) {
      return {
        isError: true,
        content: [
          {
            type: "text",
            text: `Failed to parse gptbrowser --json output: ${(e as Error).message}\nRaw output: ${raw}`,
          },
        ],
      };
    }
  }
);

server.registerTool(
  "list_models",
  {
    title: "List available models for a chatbot site",
    description:
      "List the models available for a given chatbot web UI (as reported by gptbrowser --list-models).",
    inputSchema: {
      url: z
        .string()
        .describe(
          "The chatbot site to query, e.g. chatgpt.com, gemini.com, kimi.com, chat.deepseek.com, claude.ai, perplexity.ai, chat.z.ai"
        ),
    },
  },
  async ({ url }) => {
    const result = await runGptBrowser(["--list-models", url]);

    if (result.code !== 0) {
      const message = extractErrorMessage(
        result.stderr,
        `gptbrowser exited with code ${result.code}`
      );
      return {
        isError: true,
        content: [{ type: "text", text: message }],
      };
    }

    return {
      content: [{ type: "text", text: result.stdout.trim() }],
    };
  }
);

async function main() {
  const transport = new StdioServerTransport();
  await server.connect(transport);
}

main().catch((err) => {
  console.error("gptbrowser-mcp fatal error:", err);
  process.exit(1);
});

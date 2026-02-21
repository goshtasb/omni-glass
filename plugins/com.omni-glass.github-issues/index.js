#!/usr/bin/env node
/**
 * GitHub Issues MCP server for Omni-Glass.
 *
 * Tool: create_github_issue
 *   - Takes {title, body, repo?, labels?} from the LLM args bridge
 *   - POSTs to GitHub API to create an issue
 *   - Returns issue URL on success
 *
 * Requires GITHUB_TOKEN in environment (declared in manifest).
 * Reads default_repo from plugin config file if repo arg not provided.
 *
 * Transport: NDJSON over stdio (one JSON object per line).
 */

const readline = require("readline");
const https = require("https");
const path = require("path");
const fs = require("fs");
const os = require("os");

const PLUGIN_ID = "com.omni-glass.github-issues";

const rl = readline.createInterface({
  input: process.stdin,
  output: process.stdout,
  terminal: false,
});

function send(obj) {
  process.stdout.write(JSON.stringify(obj) + "\n");
}

/** Load plugin config from ~/.config/omni-glass/plugin-config/{id}.json */
function loadConfig() {
  const configDir = path.join(
    os.platform() === "darwin"
      ? path.join(os.homedir(), "Library", "Application Support")
      : path.join(os.homedir(), ".config"),
    "omni-glass",
    "plugin-config"
  );
  const configPath = path.join(configDir, `${PLUGIN_ID}.json`);
  try {
    return JSON.parse(fs.readFileSync(configPath, "utf-8"));
  } catch {
    return {};
  }
}

/** POST to GitHub API. Returns parsed JSON response. */
function githubPost(repoPath, body, token) {
  return new Promise((resolve, reject) => {
    const data = JSON.stringify(body);
    const req = https.request(
      {
        hostname: "api.github.com",
        path: `/repos/${repoPath}/issues`,
        method: "POST",
        headers: {
          Authorization: `Bearer ${token}`,
          "Content-Type": "application/json",
          "User-Agent": "Omni-Glass-GitHub-Issues/1.0",
          Accept: "application/vnd.github+json",
          "Content-Length": Buffer.byteLength(data),
        },
      },
      (res) => {
        let body = "";
        res.on("data", (chunk) => (body += chunk));
        res.on("end", () => {
          try {
            const parsed = JSON.parse(body);
            if (res.statusCode >= 200 && res.statusCode < 300) {
              resolve(parsed);
            } else {
              reject(
                new Error(
                  parsed.message || `HTTP ${res.statusCode}: ${body.slice(0, 200)}`
                )
              );
            }
          } catch {
            reject(new Error(`Invalid response: ${body.slice(0, 200)}`));
          }
        });
      }
    );
    req.on("error", reject);
    req.write(data);
    req.end();
  });
}

function handleRequest(msg) {
  const { id, method, params } = msg;

  switch (method) {
    case "initialize":
      send({
        jsonrpc: "2.0",
        id,
        result: {
          protocolVersion: "2024-11-05",
          capabilities: { tools: {} },
          serverInfo: { name: "omni-glass-github-issues", version: "1.0.0" },
        },
      });
      break;

    case "notifications/initialized":
      break;

    case "tools/list":
      send({
        jsonrpc: "2.0",
        id,
        result: {
          tools: [
            {
              name: "create_github_issue",
              description:
                "Create a GitHub issue from snipped text. " +
                "The title and body are generated from the screen capture.",
              inputSchema: {
                type: "object",
                properties: {
                  title: {
                    type: "string",
                    description: "Issue title (concise summary)",
                  },
                  body: {
                    type: "string",
                    description: "Issue body with details, error text, context",
                  },
                  repo: {
                    type: "string",
                    description:
                      "Repository in owner/repo format (optional, uses config default)",
                  },
                  labels: {
                    type: "string",
                    description: "Comma-separated labels (optional)",
                  },
                },
                required: ["title", "body"],
              },
            },
          ],
        },
      });
      break;

    case "tools/call":
      handleToolCall(id, params);
      break;

    default:
      if (id !== undefined) {
        send({
          jsonrpc: "2.0",
          id,
          error: { code: -32601, message: `Method not found: ${method}` },
        });
      }
      break;
  }
}

async function handleToolCall(id, params) {
  const toolName = params?.name;
  const args = params?.arguments || {};

  if (toolName !== "create_github_issue") {
    send({
      jsonrpc: "2.0",
      id,
      error: { code: -32601, message: `Unknown tool: ${toolName}` },
    });
    return;
  }

  // Validate token
  const token = process.env.GITHUB_TOKEN;
  if (!token) {
    send({
      jsonrpc: "2.0",
      id,
      result: {
        content: [
          {
            type: "text",
            text: "Error: GITHUB_TOKEN not set. Add it to your .env.local file.",
          },
        ],
        isError: true,
      },
    });
    return;
  }

  // Resolve repository
  const config = loadConfig();
  const repo = args.repo || config.default_repo;
  if (!repo || !repo.includes("/")) {
    send({
      jsonrpc: "2.0",
      id,
      result: {
        content: [
          {
            type: "text",
            text:
              "Error: No repository specified. Set default_repo in plugin " +
              "settings or provide a repo argument (owner/repo format).",
          },
        ],
        isError: true,
      },
    });
    return;
  }

  // Build issue body
  const issueBody = {
    title: args.title || "New issue from Omni-Glass",
    body: args.body || "",
  };

  // Add labels if provided
  const labelStr = args.labels || config.default_labels;
  if (labelStr) {
    issueBody.labels = labelStr
      .split(",")
      .map((l) => l.trim())
      .filter(Boolean);
  }

  try {
    const result = await githubPost(repo, issueBody, token);
    send({
      jsonrpc: "2.0",
      id,
      result: {
        content: [
          {
            type: "text",
            text: `Created issue #${result.number}: ${result.title}\n${result.html_url}`,
          },
        ],
        isError: false,
      },
    });
  } catch (err) {
    send({
      jsonrpc: "2.0",
      id,
      result: {
        content: [{ type: "text", text: `GitHub API error: ${err.message}` }],
        isError: true,
      },
    });
  }
}

rl.on("line", (line) => {
  const trimmed = line.trim();
  if (!trimmed) return;
  try {
    handleRequest(JSON.parse(trimmed));
  } catch (e) {
    process.stderr.write(`[github-issues] Parse error: ${e.message}\n`);
  }
});

rl.on("close", () => process.exit(0));

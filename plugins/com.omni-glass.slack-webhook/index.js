#!/usr/bin/env node
/**
 * Slack Webhook MCP server for Omni-Glass.
 *
 * Tool: send_to_slack
 *   - Takes {text, channel?} from the LLM args bridge
 *   - POSTs to a Slack incoming webhook
 *   - Returns confirmation or error
 *
 * Requires SLACK_WEBHOOK_URL in environment (declared in manifest).
 * Validates webhook URL format at initialization.
 * Truncates messages to 3 000 chars to avoid silent Slack rejections.
 *
 * Transport: NDJSON over stdio (one JSON object per line).
 */

const readline = require("readline");
const https = require("https");

const SERVER_NAME = "omni-glass-slack-webhook";
const SERVER_VERSION = "1.0.0";
const MAX_TEXT_LENGTH = 3000;
const WEBHOOK_PATTERN =
  /^https:\/\/hooks\.slack\.com\/services\/T[A-Z0-9]+\/B[A-Z0-9]+\/[A-Za-z0-9]+$/;

const rl = readline.createInterface({
  input: process.stdin,
  output: process.stdout,
  terminal: false,
});

function send(obj) {
  process.stdout.write(JSON.stringify(obj) + "\n");
}

/** Validate SLACK_WEBHOOK_URL at startup. Returns null if valid, error obj if not. */
function validateWebhookUrl() {
  const url = process.env.SLACK_WEBHOOK_URL;
  if (!url) {
    return {
      code: -32602,
      message:
        "SLACK_WEBHOOK_URL is missing or invalid. " +
        "Expected format: https://hooks.slack.com/services/TXXXXX/BXXXXX/xxxxxxxx",
    };
  }
  if (!WEBHOOK_PATTERN.test(url)) {
    return {
      code: -32602,
      message:
        `SLACK_WEBHOOK_URL has invalid format: "${url}". ` +
        "Expected format: https://hooks.slack.com/services/TXXXXX/BXXXXX/xxxxxxxx",
    };
  }
  return null;
}

/** Truncate text to MAX_TEXT_LENGTH, appending a notice if truncated. */
function truncateText(text) {
  if (text.length <= MAX_TEXT_LENGTH) return text;
  const originalLength = text.length;
  return (
    text.slice(0, MAX_TEXT_LENGTH) +
    `\n\n... [truncated — full content was ${originalLength} chars]`
  );
}

/** POST payload to Slack webhook. Resolves on 200, rejects otherwise. */
function slackPost(payload) {
  return new Promise((resolve, reject) => {
    const webhookUrl = new URL(process.env.SLACK_WEBHOOK_URL);
    const data = JSON.stringify(payload);
    const req = https.request(
      {
        hostname: webhookUrl.hostname,
        path: webhookUrl.pathname,
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Content-Length": Buffer.byteLength(data),
        },
      },
      (res) => {
        let body = "";
        res.on("data", (chunk) => (body += chunk));
        res.on("end", () => {
          if (res.statusCode >= 200 && res.statusCode < 300) {
            resolve(body);
          } else {
            reject(
              new Error(
                `HTTP ${res.statusCode}: ${body.slice(0, 200)}`
              )
            );
          }
        });
      }
    );
    req.on("error", reject);
    req.write(data);
    req.end();
  });
}

async function handleToolCall(name, args) {
  if (name !== "send_to_slack") {
    throw new Error(`Unknown tool: ${name}`);
  }

  const text = args.text;
  if (!text) {
    return {
      content: [{ type: "text", text: "Error: text is required." }],
      isError: true,
    };
  }

  const payload = { text: truncateText(text) };
  if (args.channel) {
    payload.channel = args.channel;
  }

  try {
    await slackPost(payload);
    return {
      content: [
        {
          type: "text",
          text: `Message sent to Slack${args.channel ? ` (#${args.channel})` : ""}.`,
        },
      ],
      isError: false,
    };
  } catch (err) {
    return {
      content: [{ type: "text", text: `Slack API error: ${err.message}` }],
      isError: true,
    };
  }
}

// ═══════════════════════════════════════════════════════════════════
// MCP Server — JSON-RPC 2.0 over stdio
// ═══════════════════════════════════════════════════════════════════

function handleRequest(msg) {
  const { id, method, params } = msg;

  switch (method) {
    case "initialize": {
      const configError = validateWebhookUrl();
      if (configError) {
        send({ jsonrpc: "2.0", id, error: configError });
        return;
      }
      send({
        jsonrpc: "2.0",
        id,
        result: {
          protocolVersion: "2024-11-05",
          capabilities: { tools: {} },
          serverInfo: { name: SERVER_NAME, version: SERVER_VERSION },
        },
      });
      break;
    }

    case "notifications/initialized":
      break;

    case "tools/list":
      send({
        jsonrpc: "2.0",
        id,
        result: {
          tools: [
            {
              name: "send_to_slack",
              description:
                "Send snipped content to a Slack channel via incoming webhook. " +
                "The text is summarized by the LLM before sending.",
              inputSchema: {
                type: "object",
                properties: {
                  text: {
                    type: "string",
                    description: "Message text",
                  },
                  channel: {
                    type: "string",
                    description: "Channel name (optional)",
                  },
                },
                required: ["text"],
              },
            },
          ],
        },
      });
      break;

    case "tools/call":
      handleToolCall(params.name, params.arguments || {})
        .then((result) => send({ jsonrpc: "2.0", id, result }))
        .catch((err) =>
          send({
            jsonrpc: "2.0",
            id,
            result: {
              content: [{ type: "text", text: `Error: ${err.message}` }],
              isError: true,
            },
          })
        );
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

rl.on("line", (line) => {
  const trimmed = line.trim();
  if (!trimmed) return;
  try {
    handleRequest(JSON.parse(trimmed));
  } catch (e) {
    process.stderr.write(`[${SERVER_NAME}] Parse error: ${e.message}\n`);
  }
});

rl.on("close", () => process.exit(0));

# Build an Omni-Glass Plugin

This guide walks you through building a plugin from scratch. By the end,
your plugin will load in Omni-Glass, appear in the action menu, and
execute when the user clicks it.

**Time:** ~10 minutes. **Prerequisites:** Node.js 18+, Omni-Glass v0.3.0+.

---

## 1. Create the plugin directory

Plugins live in `~/.config/omni-glass/plugins/`. Each plugin gets its own
folder named with a reverse-domain ID:

```bash
mkdir -p ~/.config/omni-glass/plugins/com.your-name.your-plugin
cd ~/.config/omni-glass/plugins/com.your-name.your-plugin
```

Or copy the template from the Omni-Glass repo:

```bash
cp -r /path/to/omni-glass/plugins/template/ \
  ~/.config/omni-glass/plugins/com.your-name.your-plugin/
```

## 2. Define the manifest

Create `omni-glass.plugin.json`. This tells Omni-Glass what your plugin
does and what permissions it needs:

```json
{
  "id": "com.your-name.weather",
  "name": "Weather Lookup",
  "version": "0.1.0",
  "description": "Look up current weather for a location",
  "runtime": "node",
  "entry": "index.js",
  "permissions": {
    "network": ["api.openweathermap.org"],
    "environment": ["OPENWEATHER_API_KEY"],
    "clipboard": false
  }
}
```

**Required fields:** `id`, `name`, `version`, `description`, `runtime`, `entry`.

**Permissions** — declare only what you need:

| Permission | Format | What it grants |
|-----------|--------|----------------|
| `network` | `["domain1.com", "domain2.com"]` | HTTPS to listed domains |
| `environment` | `["MY_API_KEY"]` | Read specific env vars |
| `clipboard` | `true` | Read/write system clipboard |
| `filesystem` | `[{"path": "~/Documents", "access": "read"}]` | File access |
| `shell` | `{"commands": ["git"]}` | Run specific commands |

Users approve these permissions when the plugin first loads.

## 3. Define your tools

Tools are what the LLM offers to the user. Define them in `index.js`
with clear descriptions — the LLM reads these to decide when to use
your tool:

```javascript
const TOOLS = [
  {
    name: "get_weather",
    description:
      "Look up current weather for a city or location. " +
      "Use when the user snips or types a location name.",
    inputSchema: {
      type: "object",
      properties: {
        location: {
          type: "string",
          description: "City name or location",
        },
      },
      required: ["location"],
    },
  },
];
```

The `inputSchema` defines what arguments the LLM generates. Omni-Glass
uses an LLM-to-tool-args bridge to transform the user's text into
structured JSON matching your schema.

## 4. Implement the handler

In `index.js`, implement your tool logic in `handleToolCall()`:

```javascript
async function handleToolCall(name, args) {
  if (name === "get_weather") {
    const key = process.env.OPENWEATHER_API_KEY;
    if (!key) {
      return {
        content: [{ type: "text", text: "Error: OPENWEATHER_API_KEY not set." }],
        isError: true,
      };
    }

    const url = `https://api.openweathermap.org/data/2.5/weather?q=${
      encodeURIComponent(args.location)}&appid=${key}&units=metric`;

    const resp = await fetch(url);
    const data = await resp.json();

    if (data.cod !== 200) {
      return {
        content: [{ type: "text", text: `Error: ${data.message}` }],
        isError: true,
      };
    }

    const text = `${data.name}: ${data.main.temp}°C, ${data.weather[0].description}`;
    return {
      content: [{ type: "text", text }],
      isError: false,
    };
  }

  throw new Error(`Unknown tool: ${name}`);
}
```

**Return format:** Always return `{ content: [{ type: "text", text }], isError }`.

## 5. Wire up the MCP boilerplate

Your plugin communicates with Omni-Glass over stdio using JSON-RPC 2.0.
Copy the boilerplate from the template — it handles `initialize`,
`tools/list`, and `tools/call` messages automatically:

```javascript
const readline = require("readline");

const rl = readline.createInterface({
  input: process.stdin,
  output: process.stdout,
  terminal: false,
});

function send(obj) {
  process.stdout.write(JSON.stringify(obj) + "\n");
}

// Handle MCP messages
rl.on("line", (line) => {
  const msg = JSON.parse(line.trim());
  switch (msg.method) {
    case "initialize":
      send({ jsonrpc: "2.0", id: msg.id, result: {
        protocolVersion: "2024-11-05",
        capabilities: { tools: {} },
        serverInfo: { name: "my-plugin", version: "0.1.0" },
      }});
      break;
    case "notifications/initialized":
      break;
    case "tools/list":
      send({ jsonrpc: "2.0", id: msg.id, result: { tools: TOOLS } });
      break;
    case "tools/call":
      handleToolCall(msg.params.name, msg.params.arguments || {})
        .then(r => send({ jsonrpc: "2.0", id: msg.id, result: r }))
        .catch(e => send({ jsonrpc: "2.0", id: msg.id, result: {
          content: [{ type: "text", text: `Error: ${e.message}` }],
          isError: true,
        }}));
      break;
  }
});
```

**Important:** Add `"type": "commonjs"` to your `package.json` since
Omni-Glass plugins use `require()`, not ES module imports.

## 6. Test standalone

Test your plugin without Omni-Glass running:

```bash
# Initialize
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | node index.js

# List tools
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' | node index.js

# Call a tool
echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
  "name":"get_weather","arguments":{"location":"London"}
}}' | node index.js
```

You should see JSON-RPC responses on stdout.

## 7. Install and run

1. Make sure your plugin is in `~/.config/omni-glass/plugins/com.your-name.your-plugin/`
2. Restart Omni-Glass
3. On first load, a permission prompt appears — approve it
4. Your tool now appears in the action menu when relevant content is snipped
5. It's also available via the text launcher (Type Command)

## Reference: Real plugin example

See the GitHub Issues plugin source for a production example:
- `~/.config/omni-glass/plugins/com.omni-glass.github-issues/`
- Calls the GitHub API with structured args (title, body, repo, labels)
- Reads config for default_repo
- Handles errors gracefully (no token, no repo, API failures)

## Reference: Plugin lifecycle

```
Omni-Glass startup
  → Scan ~/.config/omni-glass/plugins/
  → Parse each omni-glass.plugin.json
  → Check approval status
  → Approved? Spawn process in sandbox, initialize, discover tools
  → Tools registered in ToolRegistry
  → User snips screen → CLASSIFY includes plugin tools
  → User clicks plugin action → LLM generates args → tools/call
  → Plugin returns result → displayed in action menu
```

## Troubleshooting

**Plugin doesn't appear in action menu:**
- Check Omni-Glass logs for `[MCP]` messages
- Verify manifest has valid JSON and all required fields
- Make sure `index.js` responds to `tools/list`

**Permission prompt doesn't appear:**
- Delete `~/.config/omni-glass/plugin-approvals.json` and restart

**"require is not defined" error:**
- Add `"type": "commonjs"` to your `package.json`

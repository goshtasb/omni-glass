# OmniGlass Plugin Template

A fork-and-go starter for building an OmniGlass plugin. Copy this directory, change three things, and you have a working plugin.

## What you get

| File | What it is |
|---|---|
| [index.js](index.js) | The MCP server. Reads JSON-RPC 2.0 from stdin, writes responses to stdout. Boilerplate handles `initialize`, `tools/list`, `tools/call`. |
| [omni-glass.plugin.json](omni-glass.plugin.json) | The manifest. Declares the plugin's identity and the permissions it needs. |
| [package.json](package.json) | Marks the plugin as CommonJS so `require()` works. |

The boilerplate is shared with every shipped plugin in this repo — you should never have to touch the JSON-RPC code.

## The three things you change

1. **The tool name and description** in `index.js`, inside the `TOOLS` array. The LLM reads `description` to decide when to offer your tool, so be specific.

2. **The handler body** in `index.js`, inside `handleToolCall`. This is where you call your API, transform data, or do whatever the tool does.

3. **The manifest fields** in `omni-glass.plugin.json` — `id`, `name`, `description`, and the `permissions` block. Declare only the network domains, env vars, filesystem paths, or shell commands you actually need. Anything you don't declare is denied by the sandbox.

## Test it standalone

You can drive the plugin from a terminal before installing it, just to confirm it speaks the protocol:

```bash
# Initialize handshake — should reply with protocolVersion and serverInfo
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | node index.js

# List your tools
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' | node index.js

# Call a tool
echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"my_tool","arguments":{"text":"hello"}}}' | node index.js
```

Each should print a single JSON line to stdout.

## Install it

Copy your plugin directory into `~/.config/omni-glass/plugins/`:

```bash
cp -r plugins/template/ ~/.config/omni-glass/plugins/com.you.your-plugin/
# edit the three things above
```

Restart OmniGlass. On first load you will see a permission prompt listing what your plugin asked for. Approve it, and the tool appears in the action menu when relevant content is snipped.

## Where to go next

- [docs/plugin-guide.md](../../docs/plugin-guide.md) — full plugin developer guide
- [plugins/com.omni-glass.slack-webhook/](../com.omni-glass.slack-webhook/) — a shipped plugin you can read for a realistic example
- [CONTRIBUTING.md](../../CONTRIBUTING.md) — how to open a plugin PR back to this repo

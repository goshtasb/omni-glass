# Contributing to OmniGlass

OmniGlass is a macOS Tauri 2.0 desktop app that turns screen snips into executable actions through sandboxed MCP plugins. This guide tells you how to plug in.

Before changing code, please read [standards/00-manifest.md](standards/00-manifest.md) — it routes you to the architecture, coding, and documentation standards that apply to your task.

---

## Three high-leverage ways to contribute

### 1. Build a plugin

OmniGlass treats plugins as first-class. The built-in actions are a starting point; the long tail of useful integrations comes from contributors.

A plugin is a Node.js process speaking JSON-RPC 2.0 over stdio with the host app. Look at [plugins/template/](plugins/template/) for the minimal skeleton, and [plugins/com.omni-glass.slack-webhook/](plugins/com.omni-glass.slack-webhook/) for a shipped reference.

First step:

```bash
cp -r plugins/template/ plugins/com.your-name.your-plugin/
```

Then edit the three TODO points in the new directory and open a PR. See "Opening a plugin PR" below for the full walkthrough.

### 2. Test on Windows real hardware

The Rust + TypeScript build compiles cleanly on `windows-latest` in CI (see [.github/workflows/build.yml](.github/workflows/build.yml)), but no one has driven the app on a real Windows 11 machine yet. The OCR backend, sandbox layer, and tray integration all need real-hardware validation.

First step:

```bash
git clone https://github.com/goshtasb/OmniGlass.git
cd OmniGlass
npm install
npm run tauri dev
```

If it crashes, file an issue with the failing command and the full error output. If it runs, file an issue describing what works and what doesn't.

### 3. Try to break the sandbox

Every plugin runs inside a macOS `sandbox-exec` profile that denies `/Users/` by default. If you can read `~/.ssh/id_rsa` or any other home-directory file from inside an OmniGlass plugin process, that is a critical security bug.

First step: write a minimal plugin whose handler attempts to read a target file and reports the result over stdout. Install it, run it through OmniGlass, observe whether the read succeeds. See "Reporting a sandbox escape" below.

---

## Local development setup

Requirements:

- macOS 12 or later (primary platform)
- Rust toolchain via [rustup](https://rustup.rs/) (stable channel)
- Node.js 18 or later
- Xcode Command Line Tools (`xcode-select --install`)

Build commands:

```bash
git clone https://github.com/goshtasb/OmniGlass.git
cd OmniGlass
npm install
npm run tauri dev   # full app
```

Other useful scripts (from [package.json](package.json)):

```bash
npm run build       # tsc && vite build (frontend only)
npm run dev         # vite dev server only (no Tauri shell)
```

For Rust-side changes:

```bash
cd src-tauri
cargo build
cargo test
```

---

## Opening a plugin PR

A plugin lives in `plugins/<reverse-domain-id>/` and contains three files.

### `omni-glass.plugin.json` — the manifest

This is the exact schema. Do not add or rename fields:

```json
{
  "id": "com.your-name.your-plugin",
  "name": "Human readable name",
  "version": "0.1.0",
  "description": "One sentence describing what this plugin does",
  "runtime": "node",
  "entry": "index.js",
  "permissions": {
    "clipboard": false,
    "network": ["api.example.com"],
    "environment": ["YOUR_API_KEY"],
    "filesystem": [{ "path": "~/Documents", "access": "read" }],
    "shell": { "commands": ["git"] }
  }
}
```

Declare only the permissions you need. The sandbox denies everything else.

### `index.js` — the MCP server

Use the structure from [plugins/template/index.js](plugins/template/index.js). The boilerplate handles `initialize`, `tools/list`, and `tools/call` over JSON-RPC 2.0. You write the `TOOLS` array and the `handleToolCall` body.

Test standalone before opening the PR:

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | node index.js
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' | node index.js
```

Both should print one JSON line to stdout.

### `package.json` — the runtime hint

Minimum:

```json
{ "name": "your-plugin", "version": "0.1.0", "type": "commonjs" }
```

Plugins use CommonJS (`require()`), not ES modules. The `"type": "commonjs"` field is required.

### Submitting the PR

1. Branch from `main`: `git checkout -b plugin/your-name`
2. Add your plugin directory under `plugins/`
3. Run `cargo test` inside `src-tauri/` to confirm nothing breaks
4. Open the PR. Title: `feat: add <plugin name> plugin`
5. In the description, paste the standalone test output to prove the plugin loads cleanly

For deeper detail on the plugin lifecycle, see [docs/plugin-guide.md](docs/plugin-guide.md).

---

## Reporting a sandbox escape

If you can read a file under `/Users/` that the manifest did not declare, or exfiltrate an environment variable that was not in `permissions.environment`, this is a critical bug.

Open an issue tagged `security` with this template:

```
## What I read / exfiltrated
<file path, env var name, or output>

## Plugin manifest used
<paste omni-glass.plugin.json>

## Plugin code that did it
<paste the relevant snippet>

## OmniGlass version / commit
<git rev-parse HEAD>

## macOS version
<sw_vers output>
```

Do not post the contents of anything you read. Just describe that you read it. Sensitive details can be shared privately if needed — note this in the issue and the maintainer will reach out.

---

## Code style

Match the existing style. Rust files run through `rustfmt` defaults. TypeScript is plain `tsc` — no separate formatter is required by CI.

The project follows the rules in [standards/02-coding-practices.md](standards/02-coding-practices.md), most notably:

- No file (code or docs) over 300 lines
- Vertical slice architecture — cross-domain imports only through each domain's `api/`
- Functional core, imperative shell — pure logic stays free of I/O

Don't introduce new toolchains, formatters, or dependencies in a contribution. If you think one is needed, open a discussion first.

---

## Where to ask

- [GitHub Discussions](https://github.com/goshtasb/OmniGlass/discussions) — feature requests, plugin ideas, roadmap input
- [Issues](https://github.com/goshtasb/OmniGlass/issues) — bugs, sandbox escapes, plugin requests

The project is MIT licensed. By contributing you agree that your contribution is licensed under the same terms.

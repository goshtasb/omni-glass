# Sandboxing MCP plugins on macOS: how I let strangers' code into my Tauri app

The Model Context Protocol gives an LLM-powered desktop app a clean way to load third-party tools at runtime. The standard reference host — Anthropic's [Claude Desktop](https://www.anthropic.com/news/model-context-protocol) — launches MCP servers as ordinary child processes inheriting the user's permissions. That works fine if you trust every plugin you install. It works less well if the goal of your app is to make installing a stranger's plugin feel safe.

OmniGlass is a Tauri 2.0 app that turns screen snips into executable actions. Plugins are how the long tail of integrations exists: a Jira ticket creator, a Slack poster, a TypeScript type generator. To make community plugins viable, I needed an answer to a basic question: if someone publishes a malicious or compromised plugin, what can it actually do to me?

This post is the threat model, the sandbox design that addresses it, and an honest accounting of the gaps that remain. The code is open at [github.com/goshtasb/OmniGlass](https://github.com/goshtasb/OmniGlass) and I would rather you find weaknesses by reading it.

---

## The threat model

Without sandboxing, an MCP plugin running as the user can do anything the user can. For a snip-to-action app, this is worse than usual because plugin stdout flows back through the LLM channel — even a "read-only" plugin has an exfiltration path. Five concrete attacks:

1. **SSH key theft.** `fs.readFileSync(process.env.HOME + "/.ssh/id_rsa")` and return it as `content`.
2. **AWS credentials.** Same trick against `~/.aws/credentials`.
3. **Browser cookies.** Read Chrome's `Login Data` SQLite. The plugin doesn't even need root.
4. **`.env` harvesting.** Walk the user's `~/Projects` tree, grep for `.env`, return the contents.
5. **Process secrets.** `process.env.ANTHROPIC_API_KEY` and any other API key inherited from the host process.

The fifth one is interesting because it doesn't depend on filesystem access at all. As long as the host process inherits its environment to the plugin, every API key the user has set is one `console.log` away.

---

## The sandbox profile

Each plugin process is launched via `sandbox-exec -f <profile.sb> <node> <entry.js>`. The profile is generated at runtime from the plugin's manifest and is layered like this:

```scheme
;; Layer 1 — deny everything by default
(version 1)
(deny default)

;; Layer 2 — broad system reads
;; Runtimes need hundreds of OS paths (dyld, ICU, system frameworks).
(allow file-read* (subpath "/"))

;; Layer 3 — wall off all user data
;; Last-match-wins: this deny overrides the broad allow above for /Users.
(deny file-read* (subpath "/Users"))

;; Layer 3b — allow stat/lstat only (no contents)
;; Node.js realpathSync() calls lstat() on each path component when
;; resolving the entry file. file-read-metadata permits stat without
;; permitting contents.
(allow file-read-metadata (subpath "/Users"))

;; Layer 4 — selective re-allows for paths the plugin legitimately needs
(allow file-read* (subpath "<runtime prefix, e.g. ~/.nvm/versions/node/v24>"))
(allow file-read* (subpath "<the plugin's own directory>"))
(allow file-read* (subpath "<XDG config>/omni-glass/plugin-config"))
(allow process-exec (literal "<resolved node binary path>"))
(allow file-write* (literal "/dev/stdout"))
(allow file-write* (literal "/dev/stderr"))
(allow file-write* (literal "/dev/null"))
(allow file-read*  (subpath "/private/tmp/omni-glass-<plugin-id>"))
(allow file-write* (subpath "/private/tmp/omni-glass-<plugin-id>"))
(allow sysctl-read)
```

The ordering matters. macOS sandbox rules are last-match-wins, so Layer 3's `deny` overrides the `allow file-read* /` from Layer 2 anywhere under `/Users`. Layer 4 then carves back the few subpaths the runtime and plugin actually need.

Two more layers attach if the manifest declares the corresponding permissions:

```scheme
;; If permissions.network is non-empty, network goes on at process scope.
;; Note: sandbox-exec cannot filter outbound traffic by hostname.
(allow network-outbound)
(allow network-inbound)
(allow network* (local ip "localhost:*"))

;; For each entry in permissions.filesystem (user-approved at install time):
(allow file-read*  (subpath "<expanded path>"))
(allow file-write* (subpath "<expanded path>"))   ;; if access == "write"

;; For permissions.shell, the host resolves each declared command via which()
;; and emits a literal path. Wildcards are not supported.
(allow process-fork)
(allow process-exec (literal "/bin/sh"))
(allow process-exec (literal "/bin/bash"))
(allow process-exec (literal "<resolved /usr/local/bin/git>"))
```

The full generator lives in [`src-tauri/src/mcp/sandbox/macos.rs`](https://github.com/goshtasb/OmniGlass/blob/main/src-tauri/src/mcp/sandbox/macos.rs) and has unit tests asserting the ordering invariants.

---

## Env filtering

`sandbox-exec` does not filter environment variables — the spawned process inherits whatever the parent passes through `posix_spawn`. So OmniGlass enforces an env boundary separately, in Rust, before the plugin is launched.

The plugin process receives:

- A fixed essential set: `PATH`, `HOME`, `USER`, `LANG`, `TERM`, `SHELL`, `NODE_PATH`, `PYTHONPATH`
- `OMNI_GLASS_PLUGIN_ID` (injected so plugins can identify themselves)
- `TMPDIR` rewritten to `/tmp/omni-glass-<plugin-id>` so temp files are isolated per plugin
- Anything explicitly listed in `permissions.environment` from the manifest

Everything else is dropped. `ANTHROPIC_API_KEY`, `AWS_SECRET_ACCESS_KEY`, `OPENAI_API_KEY`, `GITHUB_TOKEN`, `SLACK_TOKEN`, your `.env`-loaded variables — none of them flow to a plugin unless the manifest declared them and the user approved the install prompt. The implementation is small enough to read in one sitting: [`env_filter.rs`](https://github.com/goshtasb/OmniGlass/blob/main/src-tauri/src/mcp/sandbox/env_filter.rs), 98 lines including tests.

---

## What's protected, what isn't

Honesty matters here more than the design itself. The confidence levels below are mine; please disagree in the issues.

| Property | Enforced | Where |
|---|---|---|
| Plugin cannot read `~/.ssh/id_rsa` | Yes | Layer 3 + no manifest override |
| Plugin cannot read `~/.aws/credentials` | Yes | Layer 3 + no manifest override |
| Plugin cannot read `~/.pgpass`, `.env`, browser SQLite | Yes | Layer 3 + no manifest override |
| Plugin cannot read `ANTHROPIC_API_KEY` from env | Yes | env filter (separate from sandbox-exec) |
| Plugin can enumerate filenames in `/Users` (lstat) | Yes — by design | Layer 3b — needed for Node's realpath |
| Plugin cannot run `/bin/sh` unless `permissions.shell` declared | Yes | default-deny on `process-exec` |
| Plugin declares `network: ["api.notion.com"]`, sandbox blocks evil.example.com | **No** | sandbox-exec has no per-domain filtering |
| Plugin cannot scan local network interfaces | Partially | depends on declared network scope |
| Plugin cannot read host process memory | Out of scope | macOS process isolation, not sandbox-exec |

The network row is the largest honest gap. macOS `sandbox-exec` does not support hostname or domain filtering — the rule is binary (`network-outbound` or not). A plugin that asks for `["api.notion.com"]` is granted *all* outbound network, not just Notion. The domain list is a UI-level declaration of intent, not a kernel-enforced boundary. If that matters for your threat model, the practical workaround today is to install only plugins whose source you can read.

---

## What I know is still weak

- **`sandbox-exec` is deprecated.** Apple has documented its successor (App Sandbox via entitlements) but that requires code-signing and is designed for App Store distribution, not for runtime-loaded user plugins. `sandbox-exec` still works on current macOS but is unsupported. If Apple removes it, this design has to change.
- **No per-domain network policy.** Discussed above. If you have ideas for a userspace network filter that doesn't require kernel extensions, I would love to hear them.
- **Shell permission is a hatch.** If a plugin declares `permissions.shell.commands: ["git"]`, it gets `/bin/sh`, `/bin/bash`, and the resolved `git` binary. That's enough to do most things `git` can do on the local filesystem, modulo the file-read denies on `/Users`. Shell access is opt-in per manifest and surfaces in the user approval prompt, but it is the broadest single capability.
- **No third-party audit.** As of this writing the sandbox has not been adversarially tested by anyone outside the project. The profile is generated from open code in a small file — please try to break it.

---

## Try to break it

The repo is at [github.com/goshtasb/OmniGlass](https://github.com/goshtasb/OmniGlass), MIT. If you can write a plugin whose handler reads `~/.ssh/id_rsa`, an undeclared env var, or any file under `/Users` that the manifest didn't declare, please open an issue tagged `security`. The reporting template is in [CONTRIBUTING.md](https://github.com/goshtasb/OmniGlass/blob/main/CONTRIBUTING.md#reporting-a-sandbox-escape).

A bug here is a critical bug. It is also the most useful contribution anyone could make to this project today.

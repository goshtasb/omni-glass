# Roadmap

> Last updated: 2026-05-12. The roadmap is intentionally short and concrete. If you want to influence what gets built next, the [Discussions tab](https://github.com/goshtasb/OmniGlass/discussions) is where decisions get made.

---

## Status

OmniGlass `v1.0.0-beta` on macOS. The MCP plugin system (foundation, sandbox, manifest schema, permission prompts) and the screen-snip + text-launcher pipeline are merged to `main`. The local LLM path (Qwen-2.5 via llama.cpp) is built on [feat/phase-3b-local-llm](https://github.com/goshtasb/OmniGlass/tree/feat/phase-3b-local-llm) and has not yet merged to main. Pre-built installers are not available yet — install requires a local Tauri build.

---

## Shipped on main

### Plugin system

- Phase 2A MCP foundation — manifest parsing, loader, JSON-RPC 2.0 stdio transport (commit `d21f788`)
- Phase 2C ecosystem — plugin template directory, GitHub Issues plugin, developer guide (commit `a238922`, merged in `d0a8df9`)
- Text launcher + `run_command` action + native tray menu (commit `cb149fe`)
- Sandbox lstat + plugin config reads enabled for Node.js plugins (commit `35fcca2`)

### Sandbox and security

- Phase 2B OS-level sandbox via macOS sandbox-exec, permission prompts, safety integration (commit `503aa82`, merged in `2d9b737`)
- Sandbox escape tests gated for Windows CI compatibility (commits `3e5f62c`, `427f89e`)

### Snip + execute pipeline

- Path A code-fix flow — dual-mode prompt, markdown rendering, accurate re-OCR (commit `6642f4f`)
- Robust menu polling, stale state clearing, env loading (commit `c084939`)
- CSV export persistent success panel (commit `2c020f1`)
- File export via native save dialog (commit `6620a59`)
- "Open Link" button when plugin results contain URLs (commit `7ee1ba7`)

### Documentation and structure

- Vertical slice refactor: `lib.rs` split into 4 files all under 300 lines (commit `dfce5ac`)
- Co-located READMEs for all 4 Rust domain modules (commit `74e00b8`)
- README rewrite for OmniGlass branding + plugin example fix (commits `f129539`, `24dd56a`)
- CONTRIBUTING.md (commit `71f5418`)
- Plugin template README (commit `41de570`)

### CI / release

- macOS .dmg release-build workflow (commit `f0e7cb7`)

---

## In progress

Active development is on [feat/phase-3b-local-llm](https://github.com/goshtasb/OmniGlass/tree/feat/phase-3b-local-llm). That branch is 12 commits ahead of `main` and contains:

- Phase 3B local LLM via llama.cpp (Qwen-2.5 backend) — modules in `src-tauri/src/llm/local*` and `model_manager.rs`
- Slack webhook plugin shipped (`plugins/com.omni-glass.slack-webhook/`)
- Draggable windows, command summarization, CSV export polish
- Local LLM auto-load, flat JSON salvage, local summarization

This branch needs review and merge to `main` before the local LLM features advertised in the README are actually available to users who install from `main`.

---

## Next up (next 30 days)

1. Merge `feat/phase-3b-local-llm` to `main` so the local LLM path is in the public release
2. Easy plugins from the issue tracker that grow the ecosystem and serve as references for new contributors:
   - [#11](https://github.com/goshtasb/OmniGlass/issues/11) Shell Command Suggester — snip any error, get a runnable fix
   - [#9](https://github.com/goshtasb/OmniGlass/issues/9) TypeScript Type Generator — snip an API response, generate types
   - [#6](https://github.com/goshtasb/OmniGlass/issues/6) Clipboard History — snip and save to a searchable local history
3. Pre-built signed `.dmg` installer using the existing release workflow (`.github/workflows/build-release.yml` on main)
4. Real-hardware Windows pass — the CI build is green but no one has driven it on Windows 11

---

## Wishlist

Plugin requests with open issues but no committed timeline:

- [#10](https://github.com/goshtasb/OmniGlass/issues/10) Google Calendar Quick Check
- [#8](https://github.com/goshtasb/OmniGlass/issues/8) Quick Timer / Reminder
- [#7](https://github.com/goshtasb/OmniGlass/issues/7) AWS Error Lookup
- [#5](https://github.com/goshtasb/OmniGlass/issues/5) Datadog Event
- [#4](https://github.com/goshtasb/OmniGlass/issues/4) Notion Clipper
- [#3](https://github.com/goshtasb/OmniGlass/issues/3) Jira / Linear Ticket

Larger contributions:

- Linux port — Tesseract OCR, Bubblewrap sandbox, Wayland tray support
- Adversarial sandbox audit — third-party attempt to escape `sandbox-exec` from inside a plugin

---

## Out of scope

These were considered and intentionally declined. Please don't propose them in PRs without first opening a discussion.

- **Cloud-hosted version.** OmniGlass is local-only by design. Your API keys talk directly to the provider; the project has no server component.
- **Managed plugin registry.** Plugins are user-installed from disk into `~/.config/omni-glass/plugins/`. Distribution is whatever the plugin author chooses — npm, a Gist, a GitHub release. There is no central directory.
- **Mobile app.** Snip-to-action is a desktop workflow.

---

## Where decisions are made

- Major architectural changes: open an issue with the `discussion` label or a thread in [Discussions](https://github.com/goshtasb/OmniGlass/discussions)
- Plugin requests: open an issue with the `plugin` label
- Bugs and sandbox escapes: open an issue tagged `bug` or `security`

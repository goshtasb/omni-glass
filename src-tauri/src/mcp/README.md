# mcp/ — MCP Plugin System

## Overview

The MCP module implements a Model Context Protocol client that communicates with
plugin processes over JSON-RPC 2.0 / NDJSON stdio. It enables third-party plugins
to expose tools that appear alongside built-in actions in the action menu. The
module handles plugin lifecycle (spawn → handshake → discover → call → shutdown)
and maintains a central registry of all available tools.

## Public API

| Export | Type | Description |
|---|---|---|
| `ToolRegistry` | Struct | Central store for all tools (built-in + plugin), Tauri managed state |
| `execute_plugin_tool(registry, action_id, text)` | Function | Route a tool call to a plugin's MCP server |
| `builtins::register_builtins(registry)` | Function | Register the 6 built-in actions as internal tools |
| `loader::load_plugins(registry)` | Function | Scan plugins dir, spawn servers, discover tools |
| `manifest::load_manifest(path)` | Function | Parse and validate `omni-glass.plugin.json` |

## Internal Structure

| File | Lines | Responsibility |
|---|---|---|
| `mod.rs` | ~58 | Public API re-exports, `execute_plugin_tool` bridge function |
| `types.rs` | ~120 | MCP protocol types: JSON-RPC framing, Tool, ToolResult |
| `client.rs` | ~200 | `McpServer`: spawn child, NDJSON read/write, request/response |
| `manifest.rs` | ~150 | Parse `omni-glass.plugin.json`, validate fields, unit tests |
| `registry.rs` | ~180 | `ToolRegistry`: store tools, resolve actions, call plugins |
| `loader.rs` | ~110 | Startup scan: read plugins dir, spawn, handshake, discover |
| `builtins.rs` | ~60 | Register 6 built-in actions with `plugin_id: "builtin"` |

## Dependencies

| Crate | Used For |
|---|---|
| `tokio` | Async process spawn, stdin/stdout I/O, timeouts |
| `serde` / `serde_json` | JSON-RPC message serialization |
| `dirs` | Locate `~/.config/omni-glass/plugins/` |
| `log` | Structured logging |

## Used By

| Module | Imports | Purpose |
|---|---|---|
| `lib.rs` | `ToolRegistry` | Register as Tauri managed state, spawn plugin loading |
| `pipeline.rs` | `mcp::execute_plugin_tool` | Route plugin actions from execute_action command |

## Architecture Decisions

- **NDJSON over stdio**: MCP spec 2025-06-18 uses newline-delimited JSON (not
  Content-Length like LSP). Each message is one JSON line terminated by `\n`.
- **Non-blocking startup**: Plugin loading runs in a `tauri::async_runtime::spawn`
  so it doesn't block the app's initial render or tray setup.
- **Graceful degradation**: Plugin load failures are logged and skipped — a broken
  plugin never crashes the app. All built-in tools remain available.
- **tokio::sync::Mutex over std::sync::Mutex**: The registry uses tokio's async
  Mutex because MCP server calls involve await points while holding the lock.
- **Qualified names**: Tools are stored as `"plugin_id:tool_name"` to prevent
  collisions between plugins that expose tools with the same name.

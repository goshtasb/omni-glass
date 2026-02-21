//! Tool registry — central store for all discovered tools.
//!
//! Holds both built-in tools (dispatched to internal Rust functions)
//! and plugin tools (dispatched via MCP stdio to child processes).
//! Registered as Tauri managed state so all commands can query it.

use crate::mcp::client::McpServer;
use crate::mcp::types::McpTool;
use std::collections::HashMap;
use tokio::sync::Mutex;

/// A tool registered in the system, whether built-in or from a plugin.
#[derive(Debug, Clone)]
pub struct RegisteredTool {
    /// Plugin that owns this tool ("builtin" for internal tools).
    pub plugin_id: String,
    /// Tool name as exposed by the MCP server.
    pub name: String,
    /// Human-readable label for the action menu.
    pub display_name: String,
    /// What this tool does (shown in menu or injected into LLM prompt).
    pub description: String,
    /// JSON Schema for the tool's input (optional).
    pub input_schema: Option<serde_json::Value>,
}

/// Qualified name format: "plugin_id:tool_name".
pub fn qualified_name(plugin_id: &str, tool_name: &str) -> String {
    format!("{}:{}", plugin_id, tool_name)
}

/// Central registry for all tools and their MCP server handles.
pub struct ToolRegistry {
    /// Running MCP server processes, keyed by plugin_id.
    servers: Mutex<HashMap<String, McpServer>>,
    /// All registered tools, keyed by qualified name ("plugin_id:tool_name").
    tools: Mutex<HashMap<String, RegisteredTool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            servers: Mutex::new(HashMap::new()),
            tools: Mutex::new(HashMap::new()),
        }
    }

    /// Register tools discovered from an MCP server.
    pub async fn register_plugin_tools(&self, plugin_id: &str, tools: Vec<McpTool>) {
        let mut map = self.tools.lock().await;
        for tool in tools {
            let qname = qualified_name(plugin_id, &tool.name);
            let display = tool
                .name
                .replace('_', " ")
                .split_whitespace()
                .map(|w| {
                    let mut c = w.chars();
                    match c.next() {
                        Some(first) => {
                            first.to_uppercase().to_string() + c.as_str()
                        }
                        None => String::new(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");

            map.insert(
                qname,
                RegisteredTool {
                    plugin_id: plugin_id.to_string(),
                    name: tool.name.clone(),
                    display_name: display,
                    description: tool.description.unwrap_or_default(),
                    input_schema: tool.input_schema,
                },
            );
        }
    }

    /// Register a single built-in tool (no MCP server needed).
    pub async fn register_builtin(&self, tool: RegisteredTool) {
        let qname = qualified_name(&tool.plugin_id, &tool.name);
        self.tools.lock().await.insert(qname, tool);
    }

    /// Store a running MCP server handle.
    pub async fn add_server(&self, plugin_id: String, server: McpServer) {
        self.servers.lock().await.insert(plugin_id, server);
    }

    /// Look up a tool by its qualified name.
    pub async fn get_tool(&self, qualified: &str) -> Option<RegisteredTool> {
        self.tools.lock().await.get(qualified).cloned()
    }

    /// Check if an action ID belongs to a plugin (non-builtin) tool.
    pub async fn is_plugin_action(&self, action_id: &str) -> bool {
        let tools = self.tools.lock().await;
        // Action ID from LLM may be just the tool name — search all entries
        for (qname, tool) in tools.iter() {
            if (qname == action_id || tool.name == action_id) && tool.plugin_id != "builtin" {
                return true;
            }
        }
        false
    }

    /// Find the qualified name for an action ID (handles both qualified and bare names).
    pub async fn resolve_action(&self, action_id: &str) -> Option<String> {
        let tools = self.tools.lock().await;
        // Direct match on qualified name
        if tools.contains_key(action_id) {
            return Some(action_id.to_string());
        }
        // Search by bare tool name
        for (qname, tool) in tools.iter() {
            if tool.name == action_id {
                return Some(qname.clone());
            }
        }
        None
    }

    /// Format plugin tools as ActionMenu-compatible entries for LLM prompt injection.
    ///
    /// Uses the same field names (id, label, description, icon, requiresExecution)
    /// that the CLASSIFY prompt expects, so the LLM can include them directly
    /// in its actions array response.
    pub async fn tools_for_prompt(&self) -> String {
        let tools = self.tools.lock().await;
        let plugin_tools: Vec<_> = tools
            .values()
            .filter(|t| t.plugin_id != "builtin")
            .collect();

        if plugin_tools.is_empty() {
            return String::new();
        }

        let mut out = String::new();
        for tool in &plugin_tools {
            let qname = qualified_name(&tool.plugin_id, &tool.name);
            out.push_str(&format!(
                "- id: \"{}\", label: \"{}\", description: \"{}\", icon: \"sparkles\", requiresExecution: true\n",
                qname, tool.display_name, tool.description
            ));
        }
        out
    }

    /// Get all registered tools (for debugging / settings UI).
    pub async fn all_tools(&self) -> Vec<RegisteredTool> {
        self.tools.lock().await.values().cloned().collect()
    }

    /// Shutdown all running MCP servers.
    pub async fn shutdown_all(&self) {
        let mut servers = self.servers.lock().await;
        for (id, mut server) in servers.drain() {
            log::info!("[MCP] Shutting down plugin '{}'", id);
            server.shutdown().await;
        }
    }

    /// Call a tool on a plugin's MCP server.
    /// Resolves the tool, finds the server, and dispatches the call.
    pub async fn call_plugin_tool(
        &self,
        action_id: &str,
        arguments: serde_json::Value,
    ) -> Result<crate::mcp::types::ToolResult, String> {
        // Resolve tool info
        let tool = {
            let tools = self.tools.lock().await;
            let found = tools
                .iter()
                .find(|(qname, t)| *qname == action_id || t.name == action_id);
            match found {
                Some((_, t)) => t.clone(),
                None => return Err(format!("Tool '{}' not found in registry", action_id)),
            }
        };

        // Call on the plugin's MCP server
        let mut servers = self.servers.lock().await;
        let server = servers.get_mut(&tool.plugin_id).ok_or_else(|| {
            format!("No running server for plugin '{}'", tool.plugin_id)
        })?;
        server.call_tool(&tool.name, arguments).await
    }
}

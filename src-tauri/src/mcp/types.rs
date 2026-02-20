//! MCP protocol types for JSON-RPC 2.0 over stdio.
//!
//! These types implement the subset of the Model Context Protocol (2025-06-18)
//! needed for Omni-Glass plugin communication:
//! - JSON-RPC request/response/notification framing
//! - Tool discovery (tools/list)
//! - Tool execution (tools/call)
//! - Initialize handshake

use serde::{Deserialize, Serialize};

// ── JSON-RPC 2.0 framing ────────────────────────────────────────────

/// Outgoing JSON-RPC request (client → server).
#[derive(Debug, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcRequest {
    pub fn new(id: u64, method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            method: method.into(),
            params,
        }
    }
}

/// Outgoing JSON-RPC notification (client → server, no id).
#[derive(Debug, Serialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: &'static str,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcNotification {
    pub fn new(method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0",
            method: method.into(),
            params,
        }
    }
}

/// Incoming JSON-RPC response (server → client).
#[derive(Debug, Deserialize)]
pub struct JsonRpcResponse {
    pub id: Option<u64>,
    pub result: Option<serde_json::Value>,
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC error object.
#[derive(Debug, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

impl std::fmt::Display for JsonRpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JSON-RPC error {}: {}", self.code, self.message)
    }
}

// ── MCP initialize ──────────────────────────────────────────────────

/// Client info sent during initialize handshake.
#[derive(Debug, Serialize)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

/// Capabilities the client declares (currently empty — we're a basic client).
#[derive(Debug, Serialize)]
pub struct ClientCapabilities {}

/// Parameters for the `initialize` request.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    pub protocol_version: String,
    pub capabilities: ClientCapabilities,
    pub client_info: ClientInfo,
}

/// Server info returned from initialize.
#[derive(Debug, Deserialize)]
pub struct ServerInfo {
    pub name: Option<String>,
    pub version: Option<String>,
}

// ── MCP tools ───────────────────────────────────────────────────────

/// A tool exposed by an MCP server (from tools/list response).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTool {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub input_schema: Option<serde_json::Value>,
}

/// Result from a tools/call invocation.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResult {
    pub content: Vec<ToolResultContent>,
    #[serde(default)]
    pub is_error: bool,
}

/// A single content block in a tool result.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ToolResultContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(other)]
    Unknown,
}

impl ToolResult {
    /// Extract the concatenated text from all text content blocks.
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|c| match c {
                ToolResultContent::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

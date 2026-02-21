//! MCP client — JSON-RPC 2.0 over stdio (newline-delimited JSON).
//!
//! Spawns a child process implementing the MCP server protocol,
//! communicates via NDJSON on stdin/stdout, and provides typed
//! methods for the initialize → tools/list → tools/call lifecycle.

use crate::mcp::types::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::{Child, ChildStdin, ChildStdout};

/// Default timeout for any single JSON-RPC request (seconds).
const REQUEST_TIMEOUT_SECS: u64 = 15;

/// An active connection to an MCP server process.
pub struct McpServer {
    pub plugin_id: String,
    child: Child,
    stdin: BufWriter<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    next_id: AtomicU64,
}

impl McpServer {
    /// Spawn a child process and prepare stdio pipes.
    ///
    /// Does NOT perform the initialize handshake — call `initialize()` after.
    /// `cwd` sets the working directory (important: sandboxed plugins wall
    /// off /Users, so CWD must be inside an allowed path).
    pub fn spawn(
        plugin_id: &str,
        command: &str,
        args: &[&str],
        env: HashMap<String, String>,
        cwd: Option<&std::path::Path>,
    ) -> Result<Self, String> {
        let mut cmd = tokio::process::Command::new(command);
        cmd.args(args)
            .envs(env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }

        let mut child = cmd.spawn().map_err(|e| {
            format!("Failed to spawn MCP server '{}' ({}): {}", plugin_id, command, e)
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| format!("No stdin for MCP server '{}'", plugin_id))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| format!("No stdout for MCP server '{}'", plugin_id))?;

        Ok(Self {
            plugin_id: plugin_id.to_string(),
            child,
            stdin: BufWriter::new(stdin),
            stdout: BufReader::new(stdout),
            next_id: AtomicU64::new(1),
        })
    }

    /// Spawn a child process inside a macOS sandbox-exec sandbox.
    ///
    /// Wraps the command in `sandbox-exec -f {profile_path}` so the kernel
    /// enforces the profile's restrictions. CWD is set to `plugin_dir` because
    /// the sandbox walls off /Users — Node.js calls getcwd() at startup.
    #[cfg(target_os = "macos")]
    pub fn spawn_sandboxed(
        plugin_id: &str,
        command: &str,
        args: &[&str],
        env: HashMap<String, String>,
        sandbox_profile_path: &std::path::Path,
        plugin_dir: &std::path::Path,
    ) -> Result<Self, String> {
        let profile_str = sandbox_profile_path.to_str()
            .ok_or("Invalid sandbox profile path")?;
        let mut sandbox_args = vec!["-f", profile_str, command];
        sandbox_args.extend(args);
        Self::spawn(plugin_id, "sandbox-exec", &sandbox_args, env, Some(plugin_dir))
    }

    /// Send the initialize handshake and notifications/initialized notification.
    pub async fn initialize(&mut self) -> Result<ServerInfo, String> {
        let params = InitializeParams {
            protocol_version: "2024-11-05".to_string(),
            capabilities: ClientCapabilities {},
            client_info: ClientInfo {
                name: "omni-glass".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        };

        let resp = self
            .request("initialize", Some(serde_json::to_value(&params).unwrap()))
            .await?;

        // MCP initialize result nests server info under "serverInfo"
        let server_info: ServerInfo = resp
            .get("serverInfo")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or(ServerInfo {
                name: None,
                version: None,
            });

        // Send initialized notification (no response expected)
        self.notify("notifications/initialized", None).await?;

        log::info!(
            "[MCP] Initialized '{}' — server: {} v{}",
            self.plugin_id,
            server_info.name.as_deref().unwrap_or("unknown"),
            server_info.version.as_deref().unwrap_or("?")
        );

        Ok(server_info)
    }

    /// Discover tools via tools/list.
    pub async fn list_tools(&mut self) -> Result<Vec<McpTool>, String> {
        let resp = self.request("tools/list", None).await?;
        let tools_obj = resp
            .get("tools")
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![]));
        let tools: Vec<McpTool> =
            serde_json::from_value(tools_obj).map_err(|e| format!("Bad tools/list: {}", e))?;

        log::info!(
            "[MCP] '{}' exposes {} tools: [{}]",
            self.plugin_id,
            tools.len(),
            tools.iter().map(|t| t.name.as_str()).collect::<Vec<_>>().join(", ")
        );

        Ok(tools)
    }

    /// Execute a tool by name with the given arguments.
    pub async fn call_tool(
        &mut self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<ToolResult, String> {
        let params = serde_json::json!({
            "name": name,
            "arguments": arguments,
        });

        let resp = self.request("tools/call", Some(params)).await?;
        let result: ToolResult =
            serde_json::from_value(resp).map_err(|e| format!("Bad tools/call result: {}", e))?;

        Ok(result)
    }

    /// Gracefully shutdown: close stdin → wait briefly → kill.
    pub async fn shutdown(&mut self) {
        // Close stdin to signal EOF
        let _ = self.stdin.shutdown().await;

        // Give the process a moment to exit gracefully
        match tokio::time::timeout(
            std::time::Duration::from_secs(3),
            self.child.wait(),
        )
        .await
        {
            Ok(Ok(status)) => {
                log::info!("[MCP] '{}' exited: {}", self.plugin_id, status);
            }
            _ => {
                log::warn!("[MCP] '{}' did not exit gracefully, killing", self.plugin_id);
                let _ = self.child.kill().await;
            }
        }
    }

    // ── Internal: JSON-RPC framing ──────────────────────────────────

    /// Send a JSON-RPC request and wait for the matching response.
    async fn request(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let req = JsonRpcRequest::new(id, method, params);

        self.send_message(&serde_json::to_value(&req).unwrap())
            .await?;

        let resp = self.read_response(id).await?;

        if let Some(err) = resp.error {
            return Err(format!("[MCP] '{}' {}: {}", self.plugin_id, method, err));
        }

        resp.result.ok_or_else(|| {
            format!(
                "[MCP] '{}' {}: response had neither result nor error",
                self.plugin_id, method
            )
        })
    }

    /// Send a JSON-RPC notification (no id, no response expected).
    async fn notify(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<(), String> {
        let notif = JsonRpcNotification::new(method, params);
        self.send_message(&serde_json::to_value(&notif).unwrap())
            .await
    }

    /// Write a single NDJSON line to the child's stdin.
    async fn send_message(&mut self, value: &serde_json::Value) -> Result<(), String> {
        let mut line = serde_json::to_string(value)
            .map_err(|e| format!("JSON serialize failed: {}", e))?;
        line.push('\n');

        self.stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|e| format!("[MCP] '{}' stdin write failed: {}", self.plugin_id, e))?;
        self.stdin
            .flush()
            .await
            .map_err(|e| format!("[MCP] '{}' stdin flush failed: {}", self.plugin_id, e))?;

        Ok(())
    }

    /// Read lines from stdout until we find a response matching the given id.
    /// Skips notifications and other non-matching messages.
    async fn read_response(&mut self, expected_id: u64) -> Result<JsonRpcResponse, String> {
        let timeout = std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS);

        tokio::time::timeout(timeout, async {
            let mut line = String::new();
            loop {
                line.clear();
                let n = self
                    .stdout
                    .read_line(&mut line)
                    .await
                    .map_err(|e| format!("[MCP] '{}' stdout read failed: {}", self.plugin_id, e))?;

                if n == 0 {
                    return Err(format!(
                        "[MCP] '{}' stdout closed (process exited?)",
                        self.plugin_id
                    ));
                }

                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                // Try to parse as a JSON-RPC response
                match serde_json::from_str::<JsonRpcResponse>(trimmed) {
                    Ok(resp) if resp.id == Some(expected_id) => return Ok(resp),
                    Ok(_) => {
                        // Non-matching id or notification — skip
                        continue;
                    }
                    Err(_) => {
                        // Not valid JSON-RPC — could be server log output, skip
                        log::debug!(
                            "[MCP] '{}' ignoring non-JSON line: {}",
                            self.plugin_id,
                            &trimmed[..trimmed.len().min(100)]
                        );
                        continue;
                    }
                }
            }
        })
        .await
        .map_err(|_| {
            format!(
                "[MCP] '{}' request timed out after {}s",
                self.plugin_id, REQUEST_TIMEOUT_SECS
            )
        })?
    }
}

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::settings::McpServerConfig;

const PROTOCOL_VERSION: &str = "2024-11-05";
const CALL_TIMEOUT: Duration = Duration::from_secs(45);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTool {
    pub server_id: String,
    pub server_name: String,
    pub name: String,
    pub qualified_name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerStatus {
    pub id: String,
    pub name: String,
    pub connected: bool,
    pub tools: Vec<McpTool>,
    pub error: Option<String>,
}

struct Process {
    command_line: String,
    child: Child,
    stdin: ChildStdin,
    receiver: Receiver<String>,
    next_id: i64,
    tools: Vec<McpTool>,
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Process {
    fn connect(config: &McpServerConfig) -> Result<Self, String> {
        let command_line = format!("{} {:?}", config.command, config.args);
        let mut command = Command::new(&config.command);
        command
            .args(&config.args)
            .envs(&config.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command
            .spawn()
            .map_err(|error| format!("无法启动 MCP 服务 {}：{error}", config.name))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "MCP 服务缺少标准输入。".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "MCP 服务缺少标准输出。".to_string())?;

        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                let Ok(line) = line else {
                    break;
                };
                if line.trim().is_empty() {
                    continue;
                }
                if sender.send(line).is_err() {
                    break;
                }
            }
        });

        let mut process = Self {
            command_line,
            child,
            stdin,
            receiver,
            next_id: 0,
            tools: Vec::new(),
        };

        process.request(
            "initialize",
            serde_json::json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {
                    "name": "CERS Academic",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        )?;
        process.notify("notifications/initialized", serde_json::json!({}))?;
        process.tools = process.load_tools(config)?;
        Ok(process)
    }

    fn request(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.next_id += 1;
        let id = self.next_id;
        let message = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });
        let line = serde_json::to_string(&message)
            .map_err(|error| format!("序列化 MCP 请求失败：{error}"))?;
        writeln!(self.stdin, "{line}").map_err(|error| format!("写入 MCP 请求失败：{error}"))?;
        self.stdin
            .flush()
            .map_err(|error| format!("写入 MCP 请求失败：{error}"))?;

        let deadline = Instant::now() + CALL_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("MCP 服务响应超时。".to_string());
            }
            let line = self
                .receiver
                .recv_timeout(remaining)
                .map_err(|_| "MCP 服务响应超时。".to_string())?;
            let value: serde_json::Value = match serde_json::from_str(&line) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if value.get("id").and_then(|item| item.as_i64()) != Some(id) {
                continue;
            }
            if let Some(error) = value.get("error") {
                let message = error
                    .get("message")
                    .and_then(|item| item.as_str())
                    .unwrap_or("MCP 服务返回错误。");
                return Err(message.to_string());
            }
            return Ok(value
                .get("result")
                .cloned()
                .unwrap_or(serde_json::Value::Null));
        }
    }

    fn notify(&mut self, method: &str, params: serde_json::Value) -> Result<(), String> {
        let message = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });
        let line = serde_json::to_string(&message)
            .map_err(|error| format!("序列化 MCP 通知失败：{error}"))?;
        writeln!(self.stdin, "{line}").map_err(|error| format!("写入 MCP 通知失败：{error}"))?;
        self.stdin
            .flush()
            .map_err(|error| format!("写入 MCP 通知失败：{error}"))
    }

    fn load_tools(&mut self, config: &McpServerConfig) -> Result<Vec<McpTool>, String> {
        let result = self.request("tools/list", serde_json::json!({}))?;
        let items = result
            .get("tools")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();
        let mut tools = Vec::new();
        for item in items {
            let name = item
                .get("name")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if name.is_empty() {
                continue;
            }
            let description = item
                .get("description")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string();
            let input_schema = item
                .get("inputSchema")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({"type": "object", "properties": {}}));
            tools.push(McpTool {
                server_id: config.id.clone(),
                server_name: config.name.clone(),
                qualified_name: qualified_name(&config.id, &name),
                name,
                description,
                input_schema,
            });
        }
        Ok(tools)
    }

    fn call(&mut self, name: &str, arguments: serde_json::Value) -> Result<String, String> {
        let result = self.request(
            "tools/call",
            serde_json::json!({ "name": name, "arguments": arguments }),
        )?;
        if result
            .get("isError")
            .and_then(|item| item.as_bool())
            .unwrap_or(false)
        {
            return Err(tool_text(&result));
        }
        Ok(tool_text(&result))
    }
}

fn tool_text(result: &serde_json::Value) -> String {
    if let Some(items) = result.get("content").and_then(|value| value.as_array()) {
        let mut output = String::new();
        for item in items {
            if let Some(text) = item.get("text").and_then(|value| value.as_str()) {
                output.push_str(text);
                output.push('\n');
            } else {
                output.push_str(&serde_json::to_string(item).unwrap_or_default());
                output.push('\n');
            }
        }
        if !output.trim().is_empty() {
            return output.trim().to_string();
        }
    }
    serde_json::to_string(&result).unwrap_or_else(|_| "MCP 工具没有返回内容。".to_string())
}

fn qualified_name(server_id: &str, tool: &str) -> String {
    let mut value = format!("mcp_{}_{}", sanitize(server_id), sanitize(tool));
    value.truncate(64);
    value
}

fn sanitize(value: &str) -> String {
    let mut output: String = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    if output.is_empty() {
        output.push('x');
    }
    output
}

#[derive(Default)]
pub struct McpRegistry {
    processes: HashMap<String, Process>,
}

impl McpRegistry {
    pub fn tools(&mut self, configs: &[McpServerConfig]) -> Result<Vec<McpTool>, String> {
        let enabled: HashMap<&str, &McpServerConfig> = configs
            .iter()
            .filter(|server| server.enabled)
            .map(|server| (server.id.as_str(), server))
            .collect();
        let stale: Vec<String> = self
            .processes
            .keys()
            .filter(|id| !enabled.contains_key(id.as_str()))
            .cloned()
            .collect();
        for id in stale {
            self.processes.remove(&id);
        }

        let mut tools = Vec::new();
        for config in enabled.values() {
            let expected = format!("{} {:?}", config.command, config.args);
            let needs_restart = self
                .processes
                .get(&config.id)
                .map(|process| process.command_line != expected)
                .unwrap_or(true);
            if needs_restart {
                self.processes.remove(&config.id);
                let process = Process::connect(config)
                    .map_err(|error| format!("MCP 服务 {} 不可用：{error}", config.name))?;
                self.processes.insert(config.id.clone(), process);
            }
            if let Some(process) = self.processes.get(&config.id) {
                tools.extend(process.tools.iter().cloned());
            }
        }
        Ok(tools)
    }

    pub fn call(
        &mut self,
        config: &McpServerConfig,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<String, String> {
        if !config.enabled {
            return Err("该 MCP 服务已停用。".to_string());
        }
        let expected = format!("{} {:?}", config.command, config.args);
        let needs_restart = self
            .processes
            .get(&config.id)
            .map(|process| process.command_line != expected)
            .unwrap_or(true);
        if needs_restart {
            self.processes.remove(&config.id);
            let process = Process::connect(config)
                .map_err(|error| format!("MCP 服务 {} 不可用：{error}", config.name))?;
            self.processes.insert(config.id.clone(), process);
        }
        let process = self
            .processes
            .get_mut(&config.id)
            .ok_or_else(|| "MCP 服务未连接。".to_string())?;
        if !process.tools.iter().any(|tool| tool.name == tool_name) {
            return Err(format!("MCP 服务没有名为 {tool_name} 的工具。"));
        }
        process.call(tool_name, arguments)
    }

    pub fn status(&mut self, config: &McpServerConfig) -> McpServerStatus {
        if !config.enabled {
            return McpServerStatus {
                id: config.id.clone(),
                name: config.name.clone(),
                connected: false,
                tools: Vec::new(),
                error: Some("服务已停用。".to_string()),
            };
        }
        let expected = format!("{} {:?}", config.command, config.args);
        let needs_restart = self
            .processes
            .get(&config.id)
            .map(|process| process.command_line != expected)
            .unwrap_or(true);
        if needs_restart {
            self.processes.remove(&config.id);
            match Process::connect(config) {
                Ok(process) => {
                    self.processes.insert(config.id.clone(), process);
                }
                Err(error) => {
                    return McpServerStatus {
                        id: config.id.clone(),
                        name: config.name.clone(),
                        connected: false,
                        tools: Vec::new(),
                        error: Some(error),
                    };
                }
            }
        }
        match self.processes.get(&config.id) {
            Some(process) => McpServerStatus {
                id: config.id.clone(),
                name: config.name.clone(),
                connected: true,
                tools: process.tools.clone(),
                error: None,
            },
            None => McpServerStatus {
                id: config.id.clone(),
                name: config.name.clone(),
                connected: false,
                tools: Vec::new(),
                error: Some("服务未连接。".to_string()),
            },
        }
    }

    pub fn shutdown(&mut self, server_id: &str) {
        self.processes.remove(server_id);
    }

    pub fn shutdown_all(&mut self) {
        self.processes.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("..")
    }

    #[test]
    fn spawns_stdio_server_and_calls_a_tool() {
        let directory = repo_root().join("services/mcp-research");
        if !directory.join("server.py").is_file() {
            return;
        }
        let config = McpServerConfig {
            id: "cers-research".to_string(),
            name: "CERS Research Tools".to_string(),
            command: "uv".to_string(),
            args: vec![
                "run".to_string(),
                "--python".to_string(),
                "3.13".to_string(),
                "--directory".to_string(),
                directory.display().to_string(),
                "python".to_string(),
                "server.py".to_string(),
            ],
            env: HashMap::new(),
            enabled: true,
            description: String::new(),
        };
        let mut registry = McpRegistry::default();
        let tools = registry
            .tools(std::slice::from_ref(&config))
            .expect("列出 MCP 工具");
        assert!(tools.iter().any(|tool| tool.name == "strip_html"));

        let output = registry
            .call(
                &config,
                "strip_html",
                serde_json::json!({"html": "<p>Hello <b>CERS</b></p>"}),
            )
            .expect("调用 MCP 工具");
        assert!(output.contains("Hello CERS"), "output={output}");
        registry.shutdown_all();
    }
}

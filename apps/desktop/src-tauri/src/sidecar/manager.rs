use std::env;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread::sleep;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::domain::SidecarStatus;
use crate::storage::now_iso;

use super::{
    apply_python_env, hide_console, locate_python, worker_paths, CHECK_TIMEOUT, PYTHON_REQUIRED,
};

#[derive(Debug, Deserialize)]
struct Handshake {
    port: u16,
    python: String,
    version: String,
}

#[derive(Default)]
pub struct SidecarManager {
    child: Option<Child>,
    port: Option<u16>,
    python: Option<String>,
    version: Option<String>,
    token: Option<String>,
    restarts: u32,
    last_error: Option<String>,
    resource_dir: Option<PathBuf>,
}

impl SidecarManager {
    /// 确保 Sidecar 正在运行；进程缺失或健康检查失败时自动重启一次。
    pub fn status(&mut self) -> SidecarStatus {
        if !self.is_healthy() {
            let had_process = self.child.is_some();
            self.stop_process();
            if had_process {
                self.restarts = self.restarts.saturating_add(1);
            }
            if let Err(error) = self.start() {
                self.last_error = Some(error);
            }
        }

        self.snapshot()
    }

    pub fn shutdown(&mut self) {
        self.stop_process();
    }

    pub fn set_resource_dir(&mut self, directory: PathBuf) {
        self.resource_dir = Some(directory);
    }

    fn frozen_binary(&self) -> Option<PathBuf> {
        if let Ok(configured) = env::var("CERS_SIDECAR_BIN") {
            let candidate = PathBuf::from(configured);
            if candidate.is_file() {
                return Some(candidate);
            }
        }

        let directory = self.resource_dir.as_ref()?;
        let name = if cfg!(windows) {
            "chengsi-worker.exe"
        } else {
            "chengsi-worker"
        };
        let candidate = directory.join("sidecar").join(name);
        candidate.is_file().then_some(candidate)
    }

    #[cfg(test)]
    fn kill_for_test(&mut self) {
        if let Some(child) = self.child.as_mut() {
            force_stop_child(child);
        }
    }

    fn is_healthy(&mut self) -> bool {
        let alive = match self.child.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(None)),
            None => false,
        };
        if !alive {
            return false;
        }

        match (self.port, self.token.as_deref()) {
            (Some(port), Some(token)) => http_request(port, token, "GET", "/health").is_ok(),
            _ => false,
        }
    }

    fn start(&mut self) -> Result<(), String> {
        let token = new_token()?;
        let (worker_dir, contracts_dir) = worker_paths();

        let mut command = if let Some(binary) = self.frozen_binary() {
            let mut command = Command::new(binary);
            command.arg("--serve");
            command
        } else {
            let python_path = locate_python().ok_or_else(|| {
                format!(
                    "未找到 Python {PYTHON_REQUIRED} 运行时，也未找到内置 Sidecar。\
                     请在仓库根目录执行 \
                     `uv sync --directory services/research-worker --extra dev`。"
                )
            })?;

            if !worker_dir.is_dir() {
                return Err(format!(
                    "未找到 Python Sidecar 目录：{}",
                    worker_dir.display()
                ));
            }

            let mut command = Command::new(python_path);
            command
                .arg("-m")
                .arg("chengsi_worker")
                .arg("--serve")
                .current_dir(&worker_dir);
            apply_python_env(&mut command, &worker_dir, &contracts_dir);
            command
        };

        command
            .env("CERS_SIDECAR_TOKEN", &token)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        hide_console(&mut command);

        let mut child = command
            .spawn()
            .map_err(|error| format!("无法启动 Python Sidecar：{error}"))?;

        let Some(stdout) = child.stdout.take() else {
            force_stop_child(&mut child);
            return Err("无法读取 Python Sidecar 握手输出。".to_string());
        };

        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            let result = reader.read_line(&mut line).map(|_| line);
            let _ = sender.send(result);
        });

        let handshake_line = match receiver.recv_timeout(CHECK_TIMEOUT) {
            Ok(Ok(line)) => line,
            Ok(Err(error)) => {
                force_stop_child(&mut child);
                return Err(format!("读取 Python Sidecar 握手失败：{error}"));
            }
            Err(_) => {
                force_stop_child(&mut child);
                return Err("等待 Python Sidecar 启动超时。".to_string());
            }
        };

        let handshake: Handshake = match serde_json::from_str(handshake_line.trim()) {
            Ok(value) => value,
            Err(error) => {
                force_stop_child(&mut child);
                return Err(format!(
                    "Python Sidecar 握手输出无法解析：{error}；原始输出：{}",
                    truncate(handshake_line.trim(), 200)
                ));
            }
        };

        if let Err(error) = http_request(handshake.port, &token, "GET", "/health") {
            force_stop_child(&mut child);
            return Err(format!("Python Sidecar 健康检查失败：{error}"));
        }

        self.child = Some(child);
        self.port = Some(handshake.port);
        self.python = Some(handshake.python);
        self.version = Some(handshake.version);
        self.token = Some(token);
        self.last_error = None;

        Ok(())
    }

    fn stop_process(&mut self) {
        let Some(mut child) = self.child.take() else {
            self.port = None;
            self.token = None;
            return;
        };

        if let (Some(port), Some(token)) = (self.port, self.token.as_deref()) {
            let _ = http_request(port, token, "POST", "/shutdown");
        }

        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => sleep(Duration::from_millis(50)),
                Ok(None) => {
                    force_stop_child(&mut child);
                    break;
                }
                Err(_) => break,
            }
        }

        self.port = None;
        self.python = None;
        self.version = None;
        self.token = None;
    }

    fn snapshot(&self) -> SidecarStatus {
        let ok = self.child.is_some() && self.last_error.is_none();
        let message = if ok {
            format!(
                "Python {} Sidecar 已启动，监听 127.0.0.1:{}。",
                self.python.as_deref().unwrap_or("3.13"),
                self.port.unwrap_or(0)
            )
        } else {
            self.last_error
                .clone()
                .unwrap_or_else(|| "Python Sidecar 未运行。".to_string())
        };

        SidecarStatus {
            ok,
            mode: "supervised".to_string(),
            python: self.python.clone(),
            version: self.version.clone(),
            port: self.port,
            pid: self.child.as_ref().map(Child::id),
            restarts: self.restarts,
            message,
            checked_at: now_iso(),
        }
    }
}

impl Drop for SidecarManager {
    fn drop(&mut self) {
        self.stop_process();
    }
}

fn force_stop_child(child: &mut Child) {
    #[cfg(windows)]
    {
        // venv 的 python.exe 可能再启动基础解释器；必须按进程树终止，避免留下孤儿服务进程。
        let pid = child.id().to_string();
        let mut command = Command::new("taskkill");
        command
            .args(["/PID", &pid, "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        hide_console(&mut command);
        let _ = command.status();
    }

    #[cfg(not(windows))]
    {
        let _ = child.kill();
    }

    let _ = child.wait();
}

fn new_token() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|error| format!("无法生成 Sidecar 会话令牌：{error}"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn http_request(port: u16, token: &str, method: &str, path: &str) -> Result<String, String> {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(800))
        .map_err(|error| format!("连接 Sidecar 失败：{error}"))?;
    let _ = stream.set_read_timeout(Some(Duration::from_millis(800)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(800)));

    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {token}\r\n\
         Content-Length: 0\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("发送 Sidecar 请求失败：{error}"))?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| format!("读取 Sidecar 响应失败：{error}"))?;

    let (head, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| "Sidecar 响应格式无效。".to_string())?;

    if head.starts_with("HTTP/1.1 200") || head.starts_with("HTTP/1.0 200") {
        Ok(body.to_string())
    } else {
        Err(format!(
            "Sidecar 返回异常：{}",
            head.lines().next().unwrap_or("未知状态")
        ))
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_health_checks_and_stops_supervised_sidecar() {
        if locate_python().is_none() {
            return;
        }

        let mut manager = SidecarManager::default();
        let status = manager.status();
        assert!(status.ok, "Sidecar 启动失败：{}", status.message);
        assert_eq!(status.mode, "supervised");
        assert!(status.port.is_some());
        assert!(status.pid.is_some());
        assert!(status.python.as_deref().unwrap_or("").starts_with("3.13."));

        let first_pid = status.pid.expect("首次启动必须有 PID");
        manager.kill_for_test();

        let restarted = manager.status();
        assert!(restarted.ok, "Sidecar 重启失败：{}", restarted.message);
        assert_eq!(restarted.restarts, 1);
        assert_ne!(restarted.pid, Some(first_pid), "重启后应产生新的 PID");

        manager.shutdown();
        assert!(manager.child.is_none());
    }

    #[test]
    fn starts_frozen_sidecar_when_resource_exists() {
        let executable =
            super::super::repo_root().join("services/research-worker/dist/chengsi-worker.exe");
        if !executable.is_file() {
            // 未执行 `npm run build:sidecar` 时跳过，不把缺失产物当失败。
            return;
        }

        let directory = tempfile::tempdir().expect("创建临时资源目录");
        let sidecar_dir = directory.path().join("sidecar");
        std::fs::create_dir_all(&sidecar_dir).expect("创建 sidecar 资源目录");
        std::fs::copy(&executable, sidecar_dir.join("chengsi-worker.exe"))
            .expect("复制 Sidecar 可执行文件");

        let mut manager = SidecarManager::default();
        manager.set_resource_dir(directory.path().to_path_buf());

        let status = manager.status();
        assert!(status.ok, "内置 Sidecar 启动失败：{}", status.message);
        assert!(status.port.is_some());
        assert!(status.python.as_deref().unwrap_or("").starts_with("3.13."));

        manager.shutdown();
        assert!(manager.child.is_none());
    }
}

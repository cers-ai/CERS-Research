mod manager;

pub use manager::SidecarManager;

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

pub(crate) const CHECK_TIMEOUT: Duration = Duration::from_secs(8);
pub(crate) const PYTHON_REQUIRED: &str = "3.13.x";

pub(crate) fn locate_python() -> Option<PathBuf> {
    if let Ok(configured) = env::var("CERS_PYTHON") {
        let candidate = PathBuf::from(configured);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    let root = repo_root();
    let candidates = [
        root.join("services/research-worker/.venv/Scripts/python.exe"),
        root.join("services/research-worker/.venv/bin/python"),
    ];
    candidates.into_iter().find(|candidate| candidate.is_file())
}

pub(crate) fn repo_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or(manifest)
}

pub(crate) fn worker_paths() -> (PathBuf, PathBuf) {
    let root = repo_root();
    (
        root.join("services/research-worker"),
        root.join("packages/contracts/python"),
    )
}

pub(crate) fn apply_python_env(command: &mut Command, worker_dir: &Path, contracts_dir: &Path) {
    command
        .env("PYTHONUTF8", "1")
        .env("PYTHONIOENCODING", "utf-8");

    if let Ok(paths) = env::join_paths([worker_dir, contracts_dir]) {
        command.env("PYTHONPATH", paths);
    }
}

#[cfg(windows)]
pub(crate) fn hide_console(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
pub(crate) fn hide_console(_command: &mut Command) {}

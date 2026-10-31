use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use super::{new_id, open_project_db, open_workspace};
use crate::domain::WorkspaceInfo;

const BACKUP_FORMAT: u32 = 1;
const MAX_BACKUP_FILES: usize = 20_000;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupManifest {
    format_version: u32,
    app: String,
    schema_version: u32,
    project_id: String,
    project_name: String,
    created_at: String,
}

pub fn create_backup(workspace: &WorkspaceInfo) -> Result<String, String> {
    let root = PathBuf::from(&workspace.workspace_path);
    if !root.is_dir() {
        return Err("项目文件夹不可访问，无法备份。".to_string());
    }

    let directory = root.join("Research Outputs").join("backups");
    fs::create_dir_all(&directory).map_err(|error| format!("创建备份目录失败：{error}"))?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let archive_path = directory.join(format!("cers-backup-{stamp}.zip"));
    let temporary_db = directory.join(format!(".snapshot-{}.sqlite", new_id("bak")));

    let snapshot_result = snapshot_database(workspace, &temporary_db);
    if let Err(error) = snapshot_result {
        let _ = fs::remove_file(&temporary_db);
        return Err(error);
    }

    let result = write_archive(workspace, &root, &temporary_db, &archive_path);
    let _ = fs::remove_file(&temporary_db);
    result?;
    Ok(archive_path.display().to_string())
}

fn snapshot_database(workspace: &WorkspaceInfo, target: &Path) -> Result<(), String> {
    let _ = fs::remove_file(target);
    let connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let escaped = target.display().to_string().replace('\'', "''");
    connection
        .execute_batch(&format!("VACUUM INTO '{escaped}'"))
        .map_err(|error| format!("生成数据库快照失败：{error}"))?;
    if !target.is_file() {
        return Err("数据库快照未生成。".to_string());
    }
    Ok(())
}

fn write_archive(
    workspace: &WorkspaceInfo,
    root: &Path,
    snapshot: &Path,
    archive_path: &Path,
) -> Result<(), String> {
    let file = fs::File::create(archive_path).map_err(|error| format!("创建备份失败：{error}"))?;
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let manifest = BackupManifest {
        format_version: BACKUP_FORMAT,
        app: "CERS Academic".to_string(),
        schema_version: workspace.schema_version,
        project_id: workspace.project_id.clone(),
        project_name: workspace
            .alias
            .clone()
            .unwrap_or_else(|| workspace.name.clone()),
        created_at: super::now_iso(),
    };
    let manifest_json = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("序列化备份清单失败：{error}"))?;
    writer
        .start_file("manifest.json", options)
        .map_err(|error| format!("写入备份清单失败：{error}"))?;
    writer
        .write_all(&manifest_json)
        .map_err(|error| format!("写入备份清单失败：{error}"))?;

    writer
        .start_file(".cers-data/project.sqlite", options)
        .map_err(|error| format!("写入数据库快照失败：{error}"))?;
    let mut database =
        fs::File::open(snapshot).map_err(|error| format!("读取数据库快照失败：{error}"))?;
    std::io::copy(&mut database, &mut writer)
        .map_err(|error| format!("写入数据库快照失败：{error}"))?;

    let mut count = 0usize;
    let walker = WalkDir::new(root).max_depth(16).follow_links(false);
    for entry in walker.into_iter().filter_entry(|entry| {
        if entry.depth() == 0 {
            return true;
        }
        let name = entry.file_name().to_string_lossy();
        !(entry.file_type().is_dir()
            && (name.starts_with('.')
                || matches!(
                    name.as_ref(),
                    "node_modules" | "target" | "dist" | "build" | "__pycache__" | ".venv"
                )))
    }) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let relative = match path.strip_prefix(root) {
            Ok(relative) => relative,
            Err(_) => continue,
        };
        let relative_text = relative
            .components()
            .filter_map(|component| match component {
                Component::Normal(value) => Some(value.to_string_lossy().to_string()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/");
        if relative_text.is_empty()
            || relative_text == ".cers.tmp"
            || relative_text.starts_with(".cers-data/")
            || relative_text.starts_with("Research Outputs/backups/")
        {
            continue;
        }
        if count >= MAX_BACKUP_FILES {
            return Err("项目文件过多，无法完成备份。".to_string());
        }
        count += 1;
        writer
            .start_file(&relative_text, options)
            .map_err(|error| format!("写入备份文件失败：{error}"))?;
        let mut source = fs::File::open(path).map_err(|error| format!("读取文件失败：{error}"))?;
        std::io::copy(&mut source, &mut writer)
            .map_err(|error| format!("写入备份文件失败：{error}"))?;
    }

    writer
        .finish()
        .map_err(|error| format!("完成备份失败：{error}"))?;
    Ok(())
}

pub fn restore_backup(archive: &Path, destination: &Path) -> Result<WorkspaceInfo, String> {
    if !archive.is_file() {
        return Err("备份文件不存在。".to_string());
    }
    if destination.exists() {
        let mut entries =
            fs::read_dir(destination).map_err(|error| format!("读取目标文件夹失败：{error}"))?;
        if entries.next().is_some() {
            return Err("目标文件夹不是空文件夹，请选择一个空目录或新目录。".to_string());
        }
    } else {
        fs::create_dir_all(destination).map_err(|error| format!("创建目标文件夹失败：{error}"))?;
    }

    let file = fs::File::open(archive).map_err(|error| format!("打开备份失败：{error}"))?;
    let mut reader =
        zip::ZipArchive::new(file).map_err(|error| format!("备份文件损坏：{error}"))?;

    let mut has_manifest = false;
    for index in 0..reader.len() {
        let mut entry = reader
            .by_index(index)
            .map_err(|error| format!("读取备份条目失败：{error}"))?;
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        if relative.is_absolute()
            || relative
                .components()
                .any(|c| matches!(c, Component::ParentDir))
        {
            return Err("备份包含不安全的路径，已停止恢复。".to_string());
        }
        let target = destination.join(&relative);
        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(|error| format!("创建恢复目录失败：{error}"))?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("创建恢复目录失败：{error}"))?;
        }
        let mut buffer = Vec::new();
        entry
            .read_to_end(&mut buffer)
            .map_err(|error| format!("读取备份内容失败：{error}"))?;
        fs::write(&target, buffer).map_err(|error| format!("写入恢复文件失败：{error}"))?;
        if relative.to_string_lossy() == "manifest.json" {
            has_manifest = true;
        }
    }
    if !has_manifest {
        return Err("备份缺少清单，无法确认来源。".to_string());
    }

    let database = destination.join(".cers-data").join("project.sqlite");
    if !database.is_file() {
        return Err("备份缺少项目数据库，无法恢复。".to_string());
    }

    open_workspace(destination).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::graph::{create_node, CreateNodeInput};

    #[test]
    fn backs_up_and_restores_a_project() {
        let source = tempfile::tempdir().expect("创建源目录");
        fs::write(source.path().join("notes.md"), "原始资料").expect("写入资料");
        let workspace = open_workspace(source.path()).expect("打开项目");
        create_node(
            &workspace,
            CreateNodeInput {
                node_type: "research".to_string(),
                title: "备份测试节点".to_string(),
                goal: Some("验证恢复".to_string()),
                x: 0.0,
                y: 0.0,
                parent_id: None,
                edge_kind: None,
            },
        )
        .expect("创建节点");

        let archive = create_backup(&workspace).expect("创建备份");
        assert!(Path::new(&archive).is_file());

        let target = tempfile::tempdir().expect("创建目标目录");
        let destination = target.path().join("restored");
        let restored = restore_backup(Path::new(&archive), &destination).expect("恢复备份");
        assert_eq!(restored.project_id, workspace.project_id);
        assert!(destination.join("notes.md").is_file());
        assert!(destination.join(".cers").is_file());

        let snapshot = crate::storage::graph::load_graph(&restored).expect("读取恢复后的科研图");
        assert!(snapshot
            .nodes
            .iter()
            .any(|node| node.title == "备份测试节点"));
    }
}

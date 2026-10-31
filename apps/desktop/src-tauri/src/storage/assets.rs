use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::Instant;

use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use walkdir::WalkDir;

use super::{now_iso, open_project_db};
use crate::domain::WorkspaceInfo;

const MAX_FILES: usize = 5_000;
const MAX_DEPTH: usize = 12;
const MAX_TEXT_BYTES: u64 = 1_048_576;
const MAX_PREVIEW_CHARS: usize = 4_000;
const MAX_SEARCH_RESULTS: usize = 200;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetItem {
    pub id: String,
    pub name: String,
    pub relative_path: String,
    pub absolute_path: String,
    pub kind: String,
    pub status: String,
    pub size_bytes: Option<u64>,
    pub modified_at: Option<String>,
    pub origin: String,
    pub created_at: String,
    pub snippet: Option<String>,
}

struct AssetRow {
    id: String,
    relative_path: String,
    kind: String,
    status: String,
    origin: String,
    created_at: String,
    snippet: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub scanned: usize,
    pub added: usize,
    pub updated: usize,
    pub missing: usize,
    pub skipped: usize,
    pub truncated: bool,
    pub duration_ms: u128,
}

pub fn scan_workspace(workspace: &WorkspaceInfo) -> Result<ScanSummary, String> {
    let started = Instant::now();
    let root = PathBuf::from(&workspace.workspace_path);
    let mut files: Vec<(String, PathBuf, &'static str, Option<String>)> = Vec::new();
    let mut skipped = 0usize;
    let mut truncated = false;

    let walker = WalkDir::new(&root)
        .max_depth(MAX_DEPTH)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            if entry.depth() == 0 {
                return true;
            }
            let name = entry.file_name().to_string_lossy();
            !(entry.file_type().is_dir() && is_excluded_component(&name))
        });

    for entry in walker {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };

        if !entry.file_type().is_file() {
            continue;
        }

        let name = entry.file_name().to_string_lossy();
        if name.starts_with('.') || name == ".cers" || name == ".cers.tmp" {
            skipped += 1;
            continue;
        }

        if files.len() >= MAX_FILES {
            truncated = true;
            break;
        }

        let path = entry.path();
        let relative = match path.strip_prefix(&root) {
            Ok(relative) => normalize_relative(relative),
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        if relative.is_empty() {
            continue;
        }

        let content = extract_text(path);
        files.push((relative, path.to_path_buf(), classify(path), content));
    }

    let mut connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("无法开始资产索引事务：{error}"))?;

    let mut seen = HashSet::new();
    let mut added = 0usize;
    let mut updated = 0usize;
    let now = now_iso();

    for (relative, _, kind, content) in &files {
        seen.insert(relative.clone());

        let existing: Option<String> = transaction
            .query_row(
                "SELECT id FROM asset WHERE project_id = ?1 AND path = ?2",
                params![workspace.project_id, relative],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| format!("读取资产索引失败：{error}"))?;

        let id = existing
            .clone()
            .unwrap_or_else(|| asset_id(&workspace.project_id, relative));
        let origin = if relative.starts_with("Research Outputs/") {
            "output"
        } else {
            "workspace"
        };

        transaction
            .execute(
                "INSERT INTO asset (id, project_id, path, kind, status, content_hash, origin, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, 'indexed', NULL, ?5, ?6, ?6)
                 ON CONFLICT(project_id, path) DO UPDATE SET
                     kind = excluded.kind,
                     status = 'indexed',
                     origin = excluded.origin,
                     updated_at = excluded.updated_at",
                params![id, workspace.project_id, relative, kind, origin, now],
            )
            .map_err(|error| format!("写入资产索引失败：{error}"))?;

        let name = Path::new(relative)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(relative);
        transaction
            .execute("DELETE FROM asset_fts WHERE asset_id = ?1", params![id])
            .map_err(|error| format!("清理全文索引失败：{error}"))?;
        transaction
            .execute(
                "INSERT INTO asset_fts (asset_id, name, content) VALUES (?1, ?2, ?3)",
                params![id, name, content.as_deref().unwrap_or("")],
            )
            .map_err(|error| format!("写入全文索引失败：{error}"))?;

        if existing.is_some() {
            updated += 1;
        } else {
            added += 1;
        }
    }

    let mut missing = 0usize;
    if !truncated {
        let existing: Vec<(String, String)> = {
            let mut statement = transaction
                .prepare("SELECT id, path FROM asset WHERE project_id = ?1")
                .map_err(|error| format!("读取资产列表失败：{error}"))?;
            let rows = statement
                .query_map(params![workspace.project_id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|error| format!("读取资产列表失败：{error}"))?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|error| format!("读取资产列表失败：{error}"))?
        };

        for (id, path) in existing {
            if !seen.contains(&path) {
                transaction
                    .execute(
                        "UPDATE asset SET status = 'missing', updated_at = ?1 WHERE id = ?2",
                        params![now, id],
                    )
                    .map_err(|error| format!("更新缺失资产失败：{error}"))?;
                missing += 1;
            }
        }
    }

    transaction
        .commit()
        .map_err(|error| format!("提交资产索引失败：{error}"))?;

    Ok(ScanSummary {
        scanned: files.len(),
        added,
        updated,
        missing,
        skipped,
        truncated,
        duration_ms: started.elapsed().as_millis(),
    })
}

pub fn list_assets(
    workspace: &WorkspaceInfo,
    query: Option<&str>,
    kind: Option<&str>,
) -> Result<Vec<AssetItem>, String> {
    if let Some(needle) = query.map(str::trim).filter(|value| !value.is_empty()) {
        return search_assets(workspace, needle, kind);
    }

    let connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let mut statement = connection
        .prepare(
            "SELECT id, path, kind, status, origin, created_at
             FROM asset WHERE project_id = ?1 ORDER BY path COLLATE NOCASE",
        )
        .map_err(|error| format!("读取资产列表失败：{error}"))?;

    let rows = statement
        .query_map(params![workspace.project_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })
        .map_err(|error| format!("读取资产列表失败：{error}"))?;

    let kind_filter = kind.filter(|value| !value.is_empty() && *value != "all");
    let root = Path::new(&workspace.workspace_path);
    let mut items = Vec::new();

    for row in rows {
        let (id, relative_path, kind, status, origin, created_at) =
            row.map_err(|error| format!("读取资产行失败：{error}"))?;
        if let Some(expected) = kind_filter {
            if expected != kind.as_str() {
                continue;
            }
        }
        items.push(build_item(
            root,
            AssetRow {
                id,
                relative_path,
                kind,
                status,
                origin,
                created_at,
                snippet: None,
            },
        ));
    }

    Ok(items)
}

pub fn get_asset_text(
    workspace: &WorkspaceInfo,
    asset_id: &str,
    limit: Option<usize>,
) -> Result<Option<String>, String> {
    let connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let row: Option<(String, Option<String>)> = connection
        .query_row(
            "SELECT a.path, f.content
             FROM asset a LEFT JOIN asset_fts f ON f.asset_id = a.id
             WHERE a.project_id = ?1 AND a.id = ?2
             LIMIT 1",
            params![workspace.project_id, asset_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| format!("读取资产正文失败：{error}"))?;

    let Some((relative_path, stored)) = row else {
        return Ok(None);
    };

    let max_chars = limit.unwrap_or(MAX_PREVIEW_CHARS);
    if let Some(content) = stored.filter(|value| !value.trim().is_empty()) {
        return Ok(Some(truncate_chars(&content, max_chars)));
    }

    let path = Path::new(&workspace.workspace_path).join(relative_path);
    Ok(extract_text(&path).map(|text| truncate_chars(&text, max_chars)))
}

pub fn asset_for_run(
    workspace: &WorkspaceInfo,
    asset_id: &str,
) -> Result<(String, String, String), String> {
    let connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    connection
        .query_row(
            "SELECT path, kind, status FROM asset WHERE project_id = ?1 AND id = ?2",
            params![workspace.project_id, asset_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|error| format!("读取数据资产失败：{error}"))?
        .ok_or_else(|| "数据资产不存在或不属于当前项目。".to_string())
}

fn search_assets(
    workspace: &WorkspaceInfo,
    needle: &str,
    kind: Option<&str>,
) -> Result<Vec<AssetItem>, String> {
    let connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let root = Path::new(&workspace.workspace_path);
    let kind_filter = kind.filter(|value| !value.is_empty() && *value != "all");

    if needle.chars().count() >= 3 {
        if let Ok(items) = fts_search(&connection, root, &workspace.project_id, needle) {
            let filtered: Vec<AssetItem> = items
                .into_iter()
                .filter(|item| kind_filter.is_none_or(|expected| expected == item.kind.as_str()))
                .collect();
            if !filtered.is_empty() {
                return Ok(filtered);
            }
        }
    }

    let items = like_search(&connection, root, &workspace.project_id, needle)?;
    Ok(items
        .into_iter()
        .filter(|item| kind_filter.is_none_or(|expected| expected == item.kind.as_str()))
        .collect())
}

fn fts_search(
    connection: &Connection,
    root: &Path,
    project_id: &str,
    needle: &str,
) -> Result<Vec<AssetItem>, String> {
    let escaped = needle.replace('"', "\"\"");
    let fts_query = format!("\"{escaped}\"");
    let mut statement = connection
        .prepare(
            "SELECT a.id, a.path, a.kind, a.status, a.origin, a.created_at,
                    snippet(asset_fts, -1, '[[[', ']]]', '…', 16)
             FROM asset_fts
             JOIN asset a ON a.id = asset_fts.asset_id
             WHERE asset_fts MATCH ?1 AND a.project_id = ?2
             ORDER BY bm25(asset_fts)
             LIMIT ?3",
        )
        .map_err(|error| format!("全文检索失败：{error}"))?;

    let rows = statement
        .query_map(
            params![fts_query, project_id, MAX_SEARCH_RESULTS as i64],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        )
        .map_err(|error| format!("全文检索失败：{error}"))?;

    let mut items = Vec::new();
    for row in rows {
        let (id, relative_path, kind, status, origin, created_at, snippet) =
            row.map_err(|error| format!("读取检索结果失败：{error}"))?;
        items.push(build_item(
            root,
            AssetRow {
                id,
                relative_path,
                kind,
                status,
                origin,
                created_at,
                snippet: Some(snippet).filter(|value| !value.trim().is_empty()),
            },
        ));
    }
    Ok(items)
}

fn like_search(
    connection: &Connection,
    root: &Path,
    project_id: &str,
    needle: &str,
) -> Result<Vec<AssetItem>, String> {
    let pattern = format!("%{needle}%");
    let mut statement = connection
        .prepare(
            "SELECT a.id, a.path, a.kind, a.status, a.origin, a.created_at
             FROM asset a
             LEFT JOIN asset_fts f ON f.asset_id = a.id
             WHERE a.project_id = ?1
               AND (a.path LIKE ?2 OR f.name LIKE ?2 OR f.content LIKE ?2)
             GROUP BY a.id
             ORDER BY a.path COLLATE NOCASE
             LIMIT ?3",
        )
        .map_err(|error| format!("检索资产失败：{error}"))?;

    let rows = statement
        .query_map(
            params![project_id, pattern, MAX_SEARCH_RESULTS as i64],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .map_err(|error| format!("检索资产失败：{error}"))?;

    let mut items = Vec::new();
    for row in rows {
        let (id, relative_path, kind, status, origin, created_at) =
            row.map_err(|error| format!("读取检索结果失败：{error}"))?;
        items.push(build_item(
            root,
            AssetRow {
                id,
                relative_path,
                kind,
                status,
                origin,
                created_at,
                snippet: None,
            },
        ));
    }
    Ok(items)
}

fn build_item(root: &Path, row: AssetRow) -> AssetItem {
    let name = Path::new(&row.relative_path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(&row.relative_path)
        .to_string();
    let absolute = root.join(&row.relative_path);
    let metadata = fs::metadata(&absolute).ok();
    let effective_status = if metadata.is_none() {
        "missing".to_string()
    } else {
        row.status
    };

    AssetItem {
        id: row.id,
        name,
        relative_path: row.relative_path,
        absolute_path: absolute.display().to_string(),
        kind: row.kind,
        status: effective_status,
        size_bytes: metadata.as_ref().map(fs::Metadata::len),
        modified_at: metadata
            .and_then(|value| value.modified().ok())
            .map(format_system_time),
        origin: row.origin,
        created_at: row.created_at,
        snippet: row.snippet,
    }
}

fn extract_text(path: &Path) -> Option<String> {
    if !is_text_like(path) {
        return None;
    }

    let file = fs::File::open(path).ok()?;
    let mut buffer = Vec::new();
    file.take(MAX_TEXT_BYTES).read_to_end(&mut buffer).ok()?;
    let text = String::from_utf8_lossy(&buffer).to_string();
    if is_html(path) {
        Some(strip_html(&text))
    } else {
        Some(text)
    }
}

fn is_text_like(path: &Path) -> bool {
    matches!(
        extension(path).as_str(),
        "txt"
            | "md"
            | "markdown"
            | "csv"
            | "tsv"
            | "json"
            | "html"
            | "htm"
            | "xml"
            | "yaml"
            | "yml"
            | "toml"
            | "sql"
            | "py"
            | "ipynb"
            | "r"
            | "rmd"
            | "js"
            | "ts"
            | "tsx"
            | "jsx"
            | "rs"
            | "java"
            | "c"
            | "cpp"
            | "h"
            | "hpp"
            | "go"
            | "jl"
            | "m"
            | "sh"
            | "ps1"
    )
}

fn is_html(path: &Path) -> bool {
    matches!(extension(path).as_str(), "html" | "htm")
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_lowercase()
}

fn strip_html(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut in_tag = false;
    for character in text.chars() {
        match character {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                output.push(' ');
            }
            _ if !in_tag => output.push(character),
            _ => {}
        }
    }
    output
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let truncated: String = value.chars().take(max_chars).collect();
    format!("{truncated}…")
}

fn is_excluded_component(name: &str) -> bool {
    name.starts_with('.')
        || matches!(
            name,
            "node_modules" | "target" | "dist" | "build" | "__pycache__" | "venv" | "env" | ".venv"
        )
}

fn normalize_relative(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn classify(path: &Path) -> &'static str {
    match extension(path).as_str() {
        "pdf" => "literature",
        "doc" | "docx" | "odt" | "rtf" | "md" | "txt" => "document",
        "csv" | "tsv" | "xlsx" | "xls" | "json" | "parquet" => "dataset",
        "py" | "ipynb" | "r" | "rmd" | "js" | "ts" | "tsx" | "jsx" | "rs" | "java" | "c"
        | "cpp" | "h" | "hpp" | "go" | "jl" | "m" | "sh" | "ps1" => "code",
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "bmp" => "image",
        _ => "other",
    }
}

fn asset_id(project_id: &str, relative_path: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in format!("{project_id}|{relative_path}").as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("asset_{hash:016x}")
}

fn format_system_time(value: std::time::SystemTime) -> String {
    let datetime: DateTime<Utc> = value.into();
    datetime.to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::open_workspace;

    #[test]
    fn scans_without_touching_originals_and_marks_missing() {
        let directory = tempfile::tempdir().expect("创建临时目录");
        fs::write(directory.path().join("paper.pdf"), b"%PDF-1.4").expect("写入 PDF 样例");
        fs::write(directory.path().join("data.csv"), b"a,b\n1,2\n").expect("写入 CSV 样例");
        fs::write(directory.path().join("script.py"), b"print(1)\n").expect("写入代码样例");
        fs::write(
            directory.path().join("notes.md"),
            "城市绿地与局地热环境的关联。\n".as_bytes(),
        )
        .expect("写入笔记样例");
        fs::create_dir_all(directory.path().join(".cers-data")).expect("创建内部目录");
        fs::write(directory.path().join(".cers-data/ignored.txt"), b"internal")
            .expect("写入内部文件");

        let before = fs::read(directory.path().join("paper.pdf")).expect("读取原始文件");
        let workspace = open_workspace(directory.path()).expect("打开项目");

        let summary = scan_workspace(&workspace).expect("扫描项目");
        assert_eq!(summary.added, 4);
        assert_eq!(summary.missing, 0);

        let assets = list_assets(&workspace, None, None).expect("列出资产");
        assert_eq!(assets.len(), 4);
        assert!(assets.iter().any(|asset| asset.kind == "literature"));
        assert!(assets.iter().any(|asset| asset.kind == "dataset"));
        assert!(assets.iter().any(|asset| asset.kind == "code"));
        assert!(assets.iter().any(|asset| asset.kind == "document"));
        assert_eq!(
            fs::read(directory.path().join("paper.pdf")).expect("再次读取原始文件"),
            before
        );

        let found = list_assets(&workspace, Some("paper"), None).expect("搜索资产");
        assert_eq!(found.len(), 1);

        let content_hit = list_assets(&workspace, Some("城市绿地"), None).expect("搜索正文");
        assert_eq!(content_hit.len(), 1);
        assert_eq!(content_hit[0].name, "notes.md");

        let short_hit = list_assets(&workspace, Some("绿地"), None).expect("短词搜索");
        assert_eq!(short_hit.len(), 1);

        let notes_id = content_hit[0].id.clone();
        let preview = get_asset_text(&workspace, &notes_id, None)
            .expect("读取正文")
            .expect("正文存在");
        assert!(preview.contains("城市绿地"));

        fs::remove_file(directory.path().join("paper.pdf")).expect("删除原始文件");
        let second = scan_workspace(&workspace).expect("再次扫描");
        assert_eq!(second.missing, 1);

        let missing = list_assets(&workspace, Some("paper"), None).expect("搜索缺失资产");
        assert_eq!(missing[0].status, "missing");
    }
}

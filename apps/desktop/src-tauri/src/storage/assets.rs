use std::collections::{HashMap, HashSet};
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
const PAGE_MARKER_PREFIX: &str = "〔澄思第 ";

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
    pub page: Option<u32>,
}

struct AssetRow {
    id: String,
    relative_path: String,
    kind: String,
    status: String,
    origin: String,
    created_at: String,
    snippet: Option<String>,
    page: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub scanned: usize,
    pub added: usize,
    pub updated: usize,
    pub relinked: usize,
    pub missing: usize,
    pub skipped: usize,
    pub truncated: bool,
    pub duration_ms: u128,
}

struct ScannedFile {
    relative: String,
    kind: &'static str,
    content: Option<String>,
    fingerprint: Option<String>,
}

pub fn scan_workspace(workspace: &WorkspaceInfo) -> Result<ScanSummary, String> {
    let started = Instant::now();
    let root = PathBuf::from(&workspace.workspace_path);
    let mut files: Vec<ScannedFile> = Vec::new();
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
            if entry.file_type().is_dir() && is_excluded_component(&name) {
                return false;
            }
            if entry.file_type().is_dir() {
                let relative = entry
                    .path()
                    .strip_prefix(&root)
                    .map(normalize_relative)
                    .unwrap_or_default();
                if relative == "Research Outputs/backups" {
                    return false;
                }
            }
            true
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
        let fingerprint = fingerprint(path, content.as_deref());
        files.push(ScannedFile {
            relative,
            kind: classify(path),
            content,
            fingerprint,
        });
    }

    let mut connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("无法开始资产索引事务：{error}"))?;

    let mut seen = HashSet::new();
    let mut relinked_ids: HashSet<String> = HashSet::new();
    let mut added = 0usize;
    let mut updated = 0usize;
    let mut relinked = 0usize;
    let now = now_iso();

    let current_paths: HashSet<&str> = files.iter().map(|file| file.relative.as_str()).collect();
    let existing_rows: Vec<(String, String, Option<String>)> = {
        let mut statement = transaction
            .prepare("SELECT id, path, content_hash FROM asset WHERE project_id = ?1")
            .map_err(|error| format!("读取资产列表失败：{error}"))?;
        let rows = statement
            .query_map(params![workspace.project_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })
            .map_err(|error| format!("读取资产列表失败：{error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("读取资产列表失败：{error}"))?
    };
    let existing_by_path: HashMap<String, String> = existing_rows
        .iter()
        .map(|(id, path, _)| (path.clone(), id.clone()))
        .collect();
    let mut relinkable: HashMap<String, Vec<String>> = HashMap::new();
    for (id, path, content_hash) in &existing_rows {
        if current_paths.contains(path.as_str()) {
            continue;
        }
        if let Some(hash) = content_hash {
            relinkable.entry(hash.clone()).or_default().push(id.clone());
        }
    }

    for file in &files {
        let relative = &file.relative;
        let kind = file.kind;
        let content = &file.content;
        let fingerprint = file.fingerprint.as_deref();
        seen.insert(relative.clone());

        let existing = existing_by_path.get(relative).cloned();
        let relocated = if existing.is_none() {
            fingerprint
                .and_then(|value| relinkable.get_mut(value))
                .and_then(Vec::pop)
        } else {
            None
        };

        if let Some(id) = relocated {
            let origin = if relative.starts_with("Research Outputs/") {
                "output"
            } else {
                "workspace"
            };
            transaction
                .execute(
                    "UPDATE asset SET path = ?1, kind = ?2, status = 'relocated',
                        content_hash = ?3, origin = ?4, updated_at = ?5
                     WHERE id = ?6",
                    params![relative, kind, fingerprint, origin, now, id],
                )
                .map_err(|error| format!("更新重定位资产失败：{error}"))?;
            relinked_ids.insert(id.clone());
            relinked += 1;

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
            continue;
        }

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
                 VALUES (?1, ?2, ?3, ?4, 'indexed', ?5, ?6, ?7, ?7)
                 ON CONFLICT(project_id, path) DO UPDATE SET
                     kind = excluded.kind,
                     status = 'indexed',
                     content_hash = excluded.content_hash,
                     origin = excluded.origin,
                     updated_at = excluded.updated_at",
                params![id, workspace.project_id, relative, kind, fingerprint, origin, now],
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
            if !seen.contains(&path) && !relinked_ids.contains(&id) {
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
        relinked,
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
                page: None,
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
                    snippet(f, -1, '[[[', ']]]', '…', 16),
                    CASE WHEN instr(f.content, ?1) > 0
                         AND instr(f.content, '〔澄思第 ') > 0 THEN
                        (length(substr(f.content, 1, instr(f.content, ?1))) -
                         length(replace(substr(f.content, 1, instr(f.content, ?1)),
                                        '〔澄思第 ', ''))) / length('〔澄思第 ')
                    END AS page
             FROM asset_fts f
             JOIN asset a ON a.id = f.asset_id
             WHERE f MATCH ?2 AND a.project_id = ?3
             ORDER BY bm25(f)
             LIMIT ?4",
        )
        .map_err(|error| format!("全文检索失败：{error}"))?;

    let rows = statement
        .query_map(
            params![needle, fts_query, project_id, MAX_SEARCH_RESULTS as i64],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, Option<i64>>(7)?,
                ))
            },
        )
        .map_err(|error| format!("全文检索失败：{error}"))?;

    let mut items = Vec::new();
    for row in rows {
        let (id, relative_path, kind, status, origin, created_at, snippet, page) =
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
                page: page.and_then(|value| u32::try_from(value).ok()),
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
            "SELECT a.id, a.path, a.kind, a.status, a.origin, a.created_at,
                    CASE WHEN instr(f.content, ?4) > 0
                         AND instr(f.content, '〔澄思第 ') > 0 THEN
                        (length(substr(f.content, 1, instr(f.content, ?4))) -
                         length(replace(substr(f.content, 1, instr(f.content, ?4)),
                                        '〔澄思第 ', ''))) / length('〔澄思第 ')
                    END AS page
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
            params![project_id, pattern, MAX_SEARCH_RESULTS as i64, needle],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<i64>>(6)?,
                ))
            },
        )
        .map_err(|error| format!("检索资产失败：{error}"))?;

    let mut items = Vec::new();
    for row in rows {
        let (id, relative_path, kind, status, origin, created_at, page) =
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
                page: page.and_then(|value| u32::try_from(value).ok()),
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
        page: row.page,
    }
}

fn fingerprint(path: &Path, content: Option<&str>) -> Option<String> {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    if let Some(text) = content {
        for byte in text.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        return Some(format!("t{hash:016x}"));
    }

    let size = fs::metadata(path).ok()?.len();
    let mut buffer = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(MAX_TEXT_BYTES)
        .read_to_end(&mut buffer)
        .ok()?;
    for byte in &buffer {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    Some(format!("b{size}-{hash:016x}"))
}

fn extract_text(path: &Path) -> Option<String> {
    match extension(path).as_str() {
        "pdf" => extract_pdf_text(path),
        "docx" => extract_docx_text(path),
        _ if is_text_like(path) => extract_plain_text(path),
        _ => None,
    }
}

fn extract_plain_text(path: &Path) -> Option<String> {
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

fn extract_pdf_text(path: &Path) -> Option<String> {
    let pages = match std::panic::catch_unwind(|| pdf_extract::extract_text_by_pages(path)) {
        Ok(Ok(pages)) => pages,
        _ => return None,
    };
    let mut output = String::new();
    let mut saw_text = false;
    for (index, page) in pages.iter().enumerate() {
        output.push_str(&format!("\n\n{PAGE_MARKER_PREFIX}{} 页〕\n\n", index + 1));
        let page = collapse_whitespace(page);
        if !page.is_empty() {
            output.push_str(&page);
            saw_text = true;
        }
        if output.chars().count() >= MAX_TEXT_BYTES as usize {
            break;
        }
    }
    saw_text.then_some(output)
}

fn extract_docx_text(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let mut document = archive.by_name("word/document.xml").ok()?;
    let mut xml = String::new();
    document.read_to_string(&mut xml).ok()?;
    let text = docx_xml_to_text(&xml);
    let text = collapse_blank_lines(&text);
    (!text.trim().is_empty()).then_some(text)
}

fn docx_xml_to_text(xml: &str) -> String {
    let mut paragraphs = Vec::new();
    for chunk in xml.split("</w:p>") {
        let mut line = String::new();
        let mut rest = chunk;
        while let Some(start) = rest.find("<w:t") {
            let after = &rest[start..];
            let Some(open_end) = after.find('>') else {
                break;
            };
            let tail = &after[open_end + 1..];
            let Some(close) = tail.find("</w:t>") else {
                break;
            };
            line.push_str(&decode_xml_entities(&tail[..close]));
            rest = &tail[close + 6..];
        }
        paragraphs.push(line.trim().to_string());
    }
    paragraphs.join("\n")
}

fn decode_xml_entities(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('&') {
        output.push_str(&rest[..start]);
        let tail = &rest[start..];
        let Some(end) = tail.find(';') else {
            output.push_str(tail);
            return output;
        };
        let entity = &tail[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|code| u32::from_str_radix(code, 16).ok())
                .or_else(|| {
                    entity
                        .strip_prefix('#')
                        .and_then(|code| code.parse::<u32>().ok())
                })
                .and_then(char::from_u32),
        };
        if let Some(character) = decoded {
            output.push(character);
        } else {
            output.push_str(&tail[..=end]);
        }
        rest = &tail[end + 1..];
    }
    output.push_str(rest);
    output
}

fn collapse_whitespace(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut blank_lines = 0usize;
    for line in value.lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            blank_lines += 1;
            continue;
        }
        if !output.is_empty() {
            output.push_str(if blank_lines > 0 { "\n\n" } else { "\n" });
        }
        output.push_str(&line);
        blank_lines = 0;
    }
    output
}

fn collapse_blank_lines(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut blank = false;
    for line in value.lines() {
        if line.trim().is_empty() {
            blank = true;
            continue;
        }
        if !output.is_empty() && blank {
            output.push('\n');
        }
        output.push_str(line.trim_end());
        output.push('\n');
        blank = false;
    }
    output
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

    #[test]
    fn relinks_moved_files_by_content_hash() {
        let directory = tempfile::tempdir().expect("创建临时目录");
        fs::write(
            directory.path().join("notes.md"),
            "城市绿地与热环境：移动后的文件仍应保留同一条资产记录。
",
        )
        .expect("写入笔记样例");

        let workspace = open_workspace(directory.path()).expect("打开项目");
        let first = scan_workspace(&workspace).expect("首次扫描");
        assert_eq!(first.added, 1);
        let before = list_assets(&workspace, None, None).expect("列出资产");
        let original_id = before[0].id.clone();

        fs::create_dir_all(directory.path().join("archive")).expect("创建归档目录");
        fs::rename(
            directory.path().join("notes.md"),
            directory.path().join("archive/renamed.md"),
        )
        .expect("移动文件");

        let second = scan_workspace(&workspace).expect("再次扫描");
        assert_eq!(second.relinked, 1);
        assert_eq!(second.added, 0);
        assert_eq!(second.missing, 0);

        let after = list_assets(&workspace, None, None).expect("列出重定位资产");
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].id, original_id);
        assert_eq!(after[0].relative_path, "archive/renamed.md");
        assert_eq!(after[0].status, "relocated");
        let text = get_asset_text(&workspace, &original_id, None)
            .expect("读取正文")
            .expect("正文存在");
        assert!(text.contains("城市绿地"));
    }

    #[test]
    fn parses_pdf_pages_and_docx_text() {
        use std::io::Write;

        let directory = tempfile::tempdir().expect("创建临时目录");
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("sample-two-pages.pdf");
        fs::copy(&fixture, directory.path().join("paper-two-pages.pdf")).expect("复制 PDF 样例");

        let docx_path = directory.path().join("notes.docx");
        let file = fs::File::create(&docx_path).expect("创建 DOCX");
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file(
                "word/document.xml",
                zip::write::SimpleFileOptions::default(),
            )
            .expect("写入 DOCX 条目");
        writer
            .write_all(
                br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:r><w:t>Urban green space</w:t></w:r><w:r><w:t xml:space="preserve"> &amp; heat</w:t></w:r></w:p>
<w:p><w:r><w:t>Second paragraph</w:t></w:r></w:p>
</w:body></w:document>"#,
            )
            .expect("写入 DOCX 正文");
        writer.finish().expect("完成 DOCX");

        let workspace = open_workspace(directory.path()).expect("打开项目");
        scan_workspace(&workspace).expect("扫描项目");

        let first = list_assets(&workspace, Some("Alpha findings"), None).expect("检索第 1 页");
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].name, "paper-two-pages.pdf");
        assert_eq!(first[0].page, Some(1));

        let second = list_assets(&workspace, Some("Beta findings"), None).expect("检索第 2 页");
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].page, Some(2));

        let docx = list_assets(&workspace, Some("heat"), None).expect("检索 DOCX");
        assert_eq!(docx.len(), 1);
        assert_eq!(docx[0].name, "notes.docx");
        assert_eq!(docx[0].page, None);

        let docx_text = get_asset_text(&workspace, &docx[0].id, None)
            .expect("读取 DOCX 正文")
            .expect("DOCX 正文存在");
        assert!(docx_text.contains("Urban green space & heat"));
        assert!(docx_text.contains("Second paragraph"));

        let pdf_text = get_asset_text(&workspace, &first[0].id, Some(20_000))
            .expect("读取 PDF 正文")
            .expect("PDF 正文存在");
        assert!(pdf_text.contains("〔澄思第 1 页〕"));
        assert!(pdf_text.contains("〔澄思第 2 页〕"));
    }
}

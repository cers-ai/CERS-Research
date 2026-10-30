use rusqlite::{named_params, params, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::{new_id, now_iso, open_project_db};
use crate::domain::WorkspaceInfo;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedSource {
    pub id: String,
    pub external_id: Option<String>,
    pub provider: String,
    pub provider_name: String,
    pub doi: Option<String>,
    pub url: Option<String>,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i64>,
    pub venue: Option<String>,
    #[serde(rename = "abstract")]
    pub abstract_text: Option<String>,
    pub coverage: String,
    pub record_type: Option<String>,
    pub retrieved_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSourceInput {
    pub provider: String,
    pub provider_name: String,
    #[serde(default)]
    pub external_id: Option<String>,
    #[serde(default)]
    pub doi: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    pub title: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub year: Option<i64>,
    #[serde(default)]
    pub venue: Option<String>,
    #[serde(default, rename = "abstract")]
    pub abstract_text: Option<String>,
    pub coverage: String,
    #[serde(default)]
    pub record_type: Option<String>,
    #[serde(default)]
    pub retrieved_at: Option<String>,
    #[serde(default)]
    pub node_id: Option<String>,
}

pub fn save_source(
    workspace: &WorkspaceInfo,
    input: SaveSourceInput,
) -> Result<SavedSource, String> {
    let title = input.title.trim();
    if title.is_empty() {
        return Err("来源标题不能为空。".to_string());
    }
    if input.provider.trim().is_empty() {
        return Err("来源提供方不能为空。".to_string());
    }
    if !matches!(
        input.coverage.as_str(),
        "metadata_only" | "abstract_only" | "fulltext" | "unavailable"
    ) {
        return Err("来源覆盖级别无效。".to_string());
    }

    let doi = normalize_doi(input.doi.as_deref());
    let dedup_key = dedup_key(&doi, title, input.external_id.as_deref(), &input.provider);
    let mut connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("无法开始来源保存事务：{error}"))?;

    let existing = load_existing(&transaction, &workspace.project_id, &dedup_key)?;
    let saved = if let Some(existing) = existing {
        let merged = merge(existing, &input, doi);
        transaction
            .execute(
                "UPDATE source_record SET
                    doi = ?1, url = ?2, title = ?3, authors_json = ?4, year = ?5,
                    venue = ?6, abstract = ?7, coverage = ?8, record_type = ?9,
                    retrieved_at = ?10
                 WHERE id = ?11",
                params![
                    merged.doi,
                    merged.url,
                    merged.title,
                    serde_json::to_string(&merged.authors)
                        .map_err(|error| format!("序列化作者失败：{error}"))?,
                    merged.year,
                    merged.venue,
                    merged.abstract_text,
                    merged.coverage,
                    merged.record_type,
                    merged.retrieved_at,
                    merged.id
                ],
            )
            .map_err(|error| format!("更新来源失败：{error}"))?;
        merged
    } else {
        let saved = SavedSource {
            id: new_id("src"),
            external_id: input.external_id,
            provider: input.provider,
            provider_name: input.provider_name,
            doi,
            url: input.url,
            title: title.to_string(),
            authors: input.authors,
            year: input.year,
            venue: input.venue,
            abstract_text: input.abstract_text,
            coverage: input.coverage,
            record_type: input.record_type,
            retrieved_at: input.retrieved_at,
            created_at: now_iso(),
        };
        transaction
            .execute(
                "INSERT INTO source_record
                    (id, project_id, provider, provider_name, external_id, dedup_key,
                     doi, url, title, authors_json, year, venue, abstract, coverage,
                     record_type, retrieved_at, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
                params![
                    saved.id,
                    workspace.project_id,
                    saved.provider,
                    saved.provider_name,
                    saved.external_id,
                    dedup_key,
                    saved.doi,
                    saved.url,
                    saved.title,
                    serde_json::to_string(&saved.authors)
                        .map_err(|error| format!("序列化作者失败：{error}"))?,
                    saved.year,
                    saved.venue,
                    saved.abstract_text,
                    saved.coverage,
                    saved.record_type,
                    saved.retrieved_at,
                    saved.created_at
                ],
            )
            .map_err(|error| format!("保存来源失败：{error}"))?;
        saved
    };

    if let Some(node_id) = input.node_id.as_deref() {
        transaction
            .execute(
                "INSERT OR IGNORE INTO source_node_link (source_id, node_id, created_at)
                 VALUES (?1, ?2, ?3)",
                params![saved.id, node_id, now_iso()],
            )
            .map_err(|error| format!("关联来源到节点失败：{error}"))?;
    }

    transaction
        .commit()
        .map_err(|error| format!("提交来源保存失败：{error}"))?;

    Ok(saved)
}

pub fn list_sources(
    workspace: &WorkspaceInfo,
    node_id: Option<&str>,
    query: Option<&str>,
) -> Result<Vec<SavedSource>, String> {
    let connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let pattern = query
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("%{value}%"));

    let mut statement = connection
        .prepare(
            "SELECT s.id, s.external_id, s.provider, s.provider_name, s.doi, s.url,
                    s.title, s.authors_json, s.year, s.venue, s.abstract, s.coverage,
                    s.record_type, s.retrieved_at, s.created_at
             FROM source_record s
             LEFT JOIN source_node_link l
                    ON l.source_id = s.id AND l.node_id = :node
             WHERE s.project_id = :project
               AND (:node IS NULL OR l.node_id IS NOT NULL)
               AND (:pattern IS NULL OR s.title LIKE :pattern OR s.doi LIKE :pattern
                    OR s.venue LIKE :pattern)
             ORDER BY s.created_at DESC, s.id DESC",
        )
        .map_err(|error| format!("读取来源失败：{error}"))?;

    let rows = statement
        .query_map(
            named_params! {
                ":project": workspace.project_id,
                ":node": node_id,
                ":pattern": pattern
            },
            row_to_source,
        )
        .map_err(|error| format!("读取来源失败：{error}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("读取来源失败：{error}"))
}

pub fn remove_source_link(
    workspace: &WorkspaceInfo,
    node_id: &str,
    source_id: &str,
) -> Result<(), String> {
    let connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    connection
        .execute(
            "DELETE FROM source_node_link
             WHERE node_id = ?1 AND source_id = ?2
               AND EXISTS (
                   SELECT 1 FROM source_record
                   WHERE id = ?2 AND project_id = ?3
               )",
            params![node_id, source_id, workspace.project_id],
        )
        .map_err(|error| format!("移除来源关联失败：{error}"))?;
    Ok(())
}

pub fn delete_source(workspace: &WorkspaceInfo, source_id: &str) -> Result<(), String> {
    let connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    connection
        .execute(
            "DELETE FROM source_record WHERE id = ?1 AND project_id = ?2",
            params![source_id, workspace.project_id],
        )
        .map_err(|error| format!("删除来源失败：{error}"))?;
    Ok(())
}

pub fn linked_sources_for_chat(
    workspace: &WorkspaceInfo,
    node_id: &str,
    limit: usize,
) -> Result<Vec<SavedSource>, String> {
    let mut sources = list_sources(workspace, Some(node_id), None)?;
    sources.truncate(limit);
    Ok(sources)
}

fn load_existing(
    transaction: &rusqlite::Transaction<'_>,
    project_id: &str,
    dedup_key: &str,
) -> Result<Option<SavedSource>, String> {
    transaction
        .query_row(
            "SELECT id, external_id, provider, provider_name, doi, url, title,
                    authors_json, year, venue, abstract, coverage, record_type,
                    retrieved_at, created_at
             FROM source_record WHERE project_id = ?1 AND dedup_key = ?2",
            params![project_id, dedup_key],
            row_to_source,
        )
        .optional()
        .map_err(|error| format!("读取已有来源失败：{error}"))
}

fn row_to_source(row: &rusqlite::Row<'_>) -> rusqlite::Result<SavedSource> {
    let authors_json: String = row.get(7)?;
    Ok(SavedSource {
        id: row.get(0)?,
        external_id: row.get(1)?,
        provider: row.get(2)?,
        provider_name: row.get(3)?,
        doi: row.get(4)?,
        url: row.get(5)?,
        title: row.get(6)?,
        authors: serde_json::from_str(&authors_json).unwrap_or_default(),
        year: row.get(8)?,
        venue: row.get(9)?,
        abstract_text: row.get(10)?,
        coverage: row.get(11)?,
        record_type: row.get(12)?,
        retrieved_at: row.get(13)?,
        created_at: row.get(14)?,
    })
}

fn merge(mut existing: SavedSource, input: &SaveSourceInput, doi: Option<String>) -> SavedSource {
    if existing.doi.is_none() {
        existing.doi = doi;
    }
    if existing.url.is_none() {
        existing.url = input.url.clone();
    }
    if existing.authors.is_empty() {
        existing.authors = input.authors.clone();
    }
    if existing.year.is_none() {
        existing.year = input.year;
    }
    if existing.venue.is_none() {
        existing.venue = input.venue.clone();
    }
    if existing.record_type.is_none() {
        existing.record_type = input.record_type.clone();
    }
    if existing.retrieved_at.is_none() {
        existing.retrieved_at = input.retrieved_at.clone();
    }
    if existing.abstract_text.is_none() {
        existing.abstract_text = input.abstract_text.clone();
    }
    if coverage_rank(&input.coverage) > coverage_rank(&existing.coverage) {
        existing.coverage = input.coverage.clone();
        if input.abstract_text.is_some() {
            existing.abstract_text = input.abstract_text.clone();
        }
    }
    existing
}

fn coverage_rank(value: &str) -> i32 {
    match value {
        "fulltext" => 3,
        "abstract_only" => 2,
        "metadata_only" => 1,
        _ => 0,
    }
}

fn normalize_doi(value: Option<&str>) -> Option<String> {
    let mut doi = value?.trim();
    if doi.is_empty() {
        return None;
    }
    for prefix in [
        "https://doi.org/",
        "http://doi.org/",
        "https://dx.doi.org/",
        "http://dx.doi.org/",
    ] {
        if doi.to_lowercase().starts_with(prefix) {
            doi = &doi[prefix.len()..];
            break;
        }
    }
    Some(doi.to_lowercase())
}

fn dedup_key(
    doi: &Option<String>,
    title: &str,
    external_id: Option<&str>,
    provider: &str,
) -> String {
    if let Some(doi) = doi {
        return format!("doi:{doi}");
    }
    let normalized: String = title
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    if !normalized.is_empty() {
        return format!("title:{normalized}");
    }
    format!("ext:{provider}:{}", external_id.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::open_workspace;

    fn input(node_id: Option<String>) -> SaveSourceInput {
        SaveSourceInput {
            provider: "openalex".to_string(),
            provider_name: "OpenAlex".to_string(),
            external_id: Some("W123".to_string()),
            doi: Some("https://doi.org/10.1000/ABC".to_string()),
            url: Some("https://example.org/paper".to_string()),
            title: "Urban heat islands".to_string(),
            authors: vec!["Ada".to_string()],
            year: Some(2024),
            venue: Some("Journal".to_string()),
            abstract_text: None,
            coverage: "metadata_only".to_string(),
            record_type: Some("article".to_string()),
            retrieved_at: Some("2026-10-29T00:00:00Z".to_string()),
            node_id,
        }
    }

    #[test]
    fn saves_deduplicates_links_and_removes_sources() {
        let directory = tempfile::tempdir().expect("创建临时目录");
        let workspace = open_workspace(directory.path()).expect("打开项目");
        let root_id = workspace.root_node_id.clone();

        let first = save_source(&workspace, input(Some(root_id.clone()))).expect("保存来源");
        let mut second_input = input(Some(root_id.clone()));
        second_input.coverage = "abstract_only".to_string();
        second_input.abstract_text = Some("An abstract.".to_string());
        let second = save_source(&workspace, second_input).expect("再次保存来源");

        assert_eq!(first.id, second.id);
        assert_eq!(second.doi.as_deref(), Some("10.1000/abc"));
        assert_eq!(second.coverage, "abstract_only");

        let linked = list_sources(&workspace, Some(&root_id), None).expect("列出节点来源");
        assert_eq!(linked.len(), 1);
        assert_eq!(linked[0].abstract_text.as_deref(), Some("An abstract."));

        let all = list_sources(&workspace, None, None).expect("列出项目来源");
        assert_eq!(all.len(), 1);

        remove_source_link(&workspace, &root_id, &first.id).expect("移除关联");
        assert!(list_sources(&workspace, Some(&root_id), None)
            .expect("列出节点来源")
            .is_empty());
        assert_eq!(
            list_sources(&workspace, None, None)
                .expect("列出项目来源")
                .len(),
            1
        );

        delete_source(&workspace, &first.id).expect("删除来源");
        assert!(list_sources(&workspace, None, None)
            .expect("列出项目来源")
            .is_empty());
    }
}

use std::fs;
use std::path::Path;

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::{new_id, now_iso, open_project_db};
use crate::domain::WorkspaceInfo;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportDocument {
    pub id: String,
    pub node_id: String,
    pub content_md: String,
    pub revision: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportRevision {
    pub id: String,
    pub revision: i64,
    pub content_md: String,
    pub created_at: String,
}

pub fn ensure_report_node(workspace: &WorkspaceInfo, node_id: &str) -> Result<(), String> {
    let connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let node_type: Option<String> = connection
        .query_row(
            "SELECT type FROM node WHERE project_id = ?1 AND id = ?2",
            params![workspace.project_id, node_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("读取节点失败：{error}"))?;

    match node_type.as_deref() {
        Some("report") => Ok(()),
        Some(_) => Err("只有报告节点可以保存报告正文。".to_string()),
        None => Err("节点不存在或不属于当前项目。".to_string()),
    }
}

pub fn get_report(workspace: &WorkspaceInfo, node_id: &str) -> Result<ReportDocument, String> {
    let connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let row: Option<(String, String, String, i64, String)> = connection
        .query_row(
            "SELECT id, node_id, content_md, revision, updated_at
             FROM report_document WHERE project_id = ?1 AND node_id = ?2",
            params![workspace.project_id, node_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()
        .map_err(|error| format!("读取报告失败：{error}"))?;

    Ok(match row {
        Some((id, node_id, content_md, revision, updated_at)) => ReportDocument {
            id,
            node_id,
            content_md,
            revision,
            updated_at,
        },
        None => ReportDocument {
            id: String::new(),
            node_id: node_id.to_string(),
            content_md: String::new(),
            revision: 0,
            updated_at: String::new(),
        },
    })
}

pub fn save_report(
    workspace: &WorkspaceInfo,
    node_id: &str,
    content_md: &str,
    base_revision: i64,
) -> Result<ReportDocument, String> {
    ensure_report_node(workspace, node_id)?;
    let mut connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("无法开始报告事务：{error}"))?;

    let existing: Option<(String, i64)> = transaction
        .query_row(
            "SELECT id, revision FROM report_document
             WHERE project_id = ?1 AND node_id = ?2",
            params![workspace.project_id, node_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| format!("读取报告版本失败：{error}"))?;

    let (report_id, current_revision) = match existing {
        Some((id, revision)) => (id, revision),
        None => (new_id("rep"), 0),
    };
    if base_revision != current_revision {
        return Err("报告正文已在其他位置更新，请先刷新或对比版本。".to_string());
    }

    let next_revision = current_revision + 1;
    let now = now_iso();
    transaction
        .execute(
            "INSERT INTO report_document
                (id, project_id, node_id, content_md, revision, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(node_id) DO UPDATE SET
                 content_md = excluded.content_md,
                 revision = excluded.revision,
                 updated_at = excluded.updated_at",
            params![
                report_id,
                workspace.project_id,
                node_id,
                content_md,
                next_revision,
                now
            ],
        )
        .map_err(|error| format!("保存报告失败：{error}"))?;

    transaction
        .execute(
            "INSERT INTO report_revision (id, report_id, revision, content_md, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![new_id("rev"), report_id, next_revision, content_md, now],
        )
        .map_err(|error| format!("保存报告版本失败：{error}"))?;

    transaction
        .commit()
        .map_err(|error| format!("提交报告失败：{error}"))?;

    get_report(workspace, node_id)
}

pub fn list_revisions(
    workspace: &WorkspaceInfo,
    node_id: &str,
) -> Result<Vec<ReportRevision>, String> {
    let connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let report_id: Option<String> = connection
        .query_row(
            "SELECT id FROM report_document WHERE project_id = ?1 AND node_id = ?2",
            params![workspace.project_id, node_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("读取报告失败：{error}"))?;

    let Some(report_id) = report_id else {
        return Ok(Vec::new());
    };

    let mut statement = connection
        .prepare(
            "SELECT id, revision, content_md, created_at
             FROM report_revision WHERE report_id = ?1
             ORDER BY revision DESC",
        )
        .map_err(|error| format!("读取报告版本失败：{error}"))?;

    let rows = statement
        .query_map(params![report_id], |row| {
            Ok(ReportRevision {
                id: row.get(0)?,
                revision: row.get(1)?,
                content_md: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(|error| format!("读取报告版本失败：{error}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("读取报告版本失败：{error}"))
}

pub fn export_markdown(workspace: &WorkspaceInfo, node_id: &str) -> Result<String, String> {
    let report = get_report(workspace, node_id)?;
    if report.content_md.trim().is_empty() {
        return Err("报告正文为空，无法导出。".to_string());
    }

    let connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let title: String = connection
        .query_row(
            "SELECT title FROM node WHERE project_id = ?1 AND id = ?2",
            params![workspace.project_id, node_id],
            |row| row.get(0),
        )
        .map_err(|error| format!("读取报告节点失败：{error}"))?;

    let directory = Path::new(&workspace.workspace_path)
        .join("Research Outputs")
        .join("reports");
    fs::create_dir_all(&directory).map_err(|error| format!("创建导出目录失败：{error}"))?;
    let filename = format!("{}-r{}.md", safe_filename(&title), report.revision);
    let path = directory.join(filename);
    fs::write(&path, report.content_md)
        .map_err(|error| format!("写入 Markdown 报告失败：{error}"))?;
    Ok(path.display().to_string())
}

fn safe_filename(value: &str) -> String {
    let mut output: String = value
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect();
    output = output.trim_matches('-').to_string();
    if output.is_empty() {
        output = "report".to_string();
    }
    output.chars().take(60).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::graph::{create_node, CreateNodeInput};
    use crate::storage::open_workspace;

    #[test]
    fn saves_versions_and_exports_markdown() {
        let directory = tempfile::tempdir().expect("创建临时目录");
        let workspace = open_workspace(directory.path()).expect("打开项目");
        let snapshot = create_node(
            &workspace,
            CreateNodeInput {
                node_type: "report".to_string(),
                title: "阶段报告".to_string(),
                goal: Some("汇总研究结果".to_string()),
                x: 10.0,
                y: 10.0,
                parent_id: None,
                edge_kind: None,
            },
        )
        .expect("创建报告节点");
        let node_id = snapshot
            .nodes
            .iter()
            .find(|node| node.title == "阶段报告")
            .map(|node| node.id.clone())
            .expect("找到报告节点");

        let first = save_report(&workspace, &node_id, "# 草稿", 0).expect("保存第一版");
        assert_eq!(first.revision, 1);

        let second = save_report(&workspace, &node_id, "# 第二版", 1).expect("保存第二版");
        assert_eq!(second.revision, 2);
        assert!(save_report(&workspace, &node_id, "# 冲突", 1).is_err());

        let revisions = list_revisions(&workspace, &node_id).expect("列出版本");
        assert_eq!(revisions.len(), 2);
        assert_eq!(revisions[0].revision, 2);

        let path = export_markdown(&workspace, &node_id).expect("导出 Markdown");
        let exported = fs::read_to_string(path).expect("读取导出文件");
        assert_eq!(exported, "# 第二版");
    }
}

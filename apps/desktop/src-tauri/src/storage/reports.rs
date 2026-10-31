use std::fs;
use std::path::Path;

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

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

pub fn export_target(
    workspace: &WorkspaceInfo,
    node_id: &str,
    format: &str,
) -> Result<(String, String, String), String> {
    if !matches!(format, "docx" | "pdf") {
        return Err("不支持的导出格式。".to_string());
    }
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
    let path = directory.join(format!(
        "{}-r{}.{}",
        safe_filename(&title),
        report.revision,
        format
    ));
    Ok((report.content_md, title, path.display().to_string()))
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportChange {
    pub id: String,
    pub old_text: String,
    pub new_text: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub review: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportChangeProposalView {
    pub id: String,
    pub node_id: String,
    pub base_revision: i64,
    pub status: String,
    pub summary: String,
    pub changes: Vec<ReportChange>,
}

pub fn insert_report_changes(
    workspace: &WorkspaceInfo,
    node_id: &str,
    base_revision: i64,
    summary: &str,
    mut changes: Vec<ReportChange>,
) -> Result<ReportChangeProposalView, String> {
    ensure_report_node(workspace, node_id)?;
    let report = get_report(workspace, node_id)?;
    if report.id.is_empty() {
        return Err("请先保存报告正文，再生成修改建议。".to_string());
    }
    for change in &mut changes {
        if change.review.is_empty() {
            change.review = "pending".to_string();
        }
    }

    let connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let id = new_id("chg");
    let now = now_iso();
    let payload = serde_json::json!({
        "summary": summary,
        "changes": changes,
    });
    connection
        .execute(
            "INSERT INTO report_change_proposal
                (id, project_id, report_id, node_id, base_revision, status,
                 payload_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6, ?7, ?7)",
            params![
                id,
                workspace.project_id,
                report.id,
                node_id,
                base_revision,
                serde_json::to_string(&payload)
                    .map_err(|error| format!("序列化修改提案失败：{error}"))?,
                now
            ],
        )
        .map_err(|error| format!("保存修改提案失败：{error}"))?;

    Ok(ReportChangeProposalView {
        id,
        node_id: node_id.to_string(),
        base_revision,
        status: "pending".to_string(),
        summary: summary.to_string(),
        changes,
    })
}

pub fn apply_report_changes(
    workspace: &WorkspaceInfo,
    node_id: &str,
    proposal_id: &str,
    accepted_ids: &[String],
) -> Result<(ReportDocument, ReportChangeProposalView), String> {
    ensure_report_node(workspace, node_id)?;
    let proposal = load_change_proposal(workspace, proposal_id)?;
    if proposal.node_id != node_id {
        return Err("修改提案不属于当前报告节点。".to_string());
    }
    if proposal.status != "pending" {
        return Err("该修改提案已处理。".to_string());
    }

    let report = get_report(workspace, node_id)?;
    if report.revision != proposal.base_revision {
        mark_proposal(workspace, proposal_id, "conflict")?;
        return Err("报告正文已变化，请重新生成修改建议。".to_string());
    }

    let accepted: std::collections::HashSet<&str> =
        accepted_ids.iter().map(String::as_str).collect();
    if accepted.is_empty() {
        return Err("至少选择一项修改。".to_string());
    }

    let mut content = report.content_md.clone();
    let mut changes = proposal.changes.clone();
    let mut applied = 0usize;

    for change in &mut changes {
        if !accepted.contains(change.id.as_str()) {
            change.review = "rejected".to_string();
            continue;
        }
        if !content.contains(&change.old_text) {
            mark_proposal(workspace, proposal_id, "conflict")?;
            return Err("待修改段落已变化，请重新核对。".to_string());
        }
        content = content.replacen(&change.old_text, &change.new_text, 1);
        change.review = "accepted".to_string();
        applied += 1;
    }

    let saved = save_report(workspace, node_id, &content, report.revision)?;
    let status = if applied == changes.len() {
        "applied"
    } else {
        "partially_applied"
    };
    mark_proposal(workspace, proposal_id, status)?;

    Ok((
        saved,
        ReportChangeProposalView {
            id: proposal.id,
            node_id: proposal.node_id,
            base_revision: proposal.base_revision,
            status: status.to_string(),
            summary: proposal.summary,
            changes,
        },
    ))
}

pub fn list_pending_change_proposals(
    workspace: &WorkspaceInfo,
    node_id: &str,
) -> Result<Vec<ReportChangeProposalView>, String> {
    let connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let mut statement = connection
        .prepare(
            "SELECT node_id, status, base_revision, payload_json, id
             FROM report_change_proposal
             WHERE project_id = ?1 AND node_id = ?2 AND status = 'pending'
             ORDER BY created_at DESC",
        )
        .map_err(|error| format!("读取待处理修改提案失败：{error}"))?;
    let rows = statement
        .query_map(params![workspace.project_id, node_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(|error| format!("读取待处理修改提案失败：{error}"))?;

    let mut proposals = Vec::new();
    for row in rows {
        let (node_id, status, base_revision, payload_json, id) =
            row.map_err(|error| format!("读取待处理修改提案失败：{error}"))?;
        proposals.push(proposal_from_payload(
            id,
            node_id,
            status,
            base_revision,
            &payload_json,
        ));
    }
    Ok(proposals)
}

fn load_change_proposal(
    workspace: &WorkspaceInfo,
    proposal_id: &str,
) -> Result<ReportChangeProposalView, String> {
    let connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let row: Option<(String, String, i64, String, String)> = connection
        .query_row(
            "SELECT node_id, status, base_revision, payload_json, id
             FROM report_change_proposal WHERE project_id = ?1 AND id = ?2",
            params![workspace.project_id, proposal_id],
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
        .map_err(|error| format!("读取修改提案失败：{error}"))?;

    let Some((node_id, status, base_revision, payload_json, id)) = row else {
        return Err("修改提案不存在。".to_string());
    };
    Ok(proposal_from_payload(
        id,
        node_id,
        status,
        base_revision,
        &payload_json,
    ))
}

fn proposal_from_payload(
    id: String,
    node_id: String,
    status: String,
    base_revision: i64,
    payload_json: &str,
) -> ReportChangeProposalView {
    let payload: serde_json::Value =
        serde_json::from_str(payload_json).unwrap_or(serde_json::Value::Null);
    let summary = payload
        .get("summary")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let changes: Vec<ReportChange> = payload
        .get("changes")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default();

    ReportChangeProposalView {
        id,
        node_id,
        base_revision,
        status,
        summary,
        changes,
    }
}

fn mark_proposal(workspace: &WorkspaceInfo, proposal_id: &str, status: &str) -> Result<(), String> {
    let connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    connection
        .execute(
            "UPDATE report_change_proposal SET status = ?1, updated_at = ?2
             WHERE project_id = ?3 AND id = ?4",
            params![status, now_iso(), workspace.project_id, proposal_id],
        )
        .map_err(|error| format!("更新修改提案状态失败：{error}"))?;
    Ok(())
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

    #[test]
    fn applies_only_accepted_report_changes() {
        let directory = tempfile::tempdir().expect("创建临时目录");
        let workspace = open_workspace(directory.path()).expect("打开项目");
        let snapshot = create_node(
            &workspace,
            CreateNodeInput {
                node_type: "report".to_string(),
                title: "修订报告".to_string(),
                goal: None,
                x: 0.0,
                y: 0.0,
                parent_id: None,
                edge_kind: None,
            },
        )
        .expect("创建报告节点");
        let node_id = snapshot
            .nodes
            .iter()
            .find(|node| node.title == "修订报告")
            .map(|node| node.id.clone())
            .expect("找到报告节点");

        let saved = save_report(
            &workspace,
            &node_id,
            "# 结论\n样本量偏小。\n方法部分需要补充。",
            0,
        )
        .expect("保存报告");

        let proposal = insert_report_changes(
            &workspace,
            &node_id,
            saved.revision,
            "收紧表述",
            vec![
                ReportChange {
                    id: "c1".to_string(),
                    old_text: "样本量偏小。".to_string(),
                    new_text: "样本量偏小，结论需谨慎解读。".to_string(),
                    reason: "补充限定".to_string(),
                    review: "pending".to_string(),
                },
                ReportChange {
                    id: "c2".to_string(),
                    old_text: "方法部分需要补充。".to_string(),
                    new_text: "方法部分需补充样本筛选流程。".to_string(),
                    reason: "细化".to_string(),
                    review: "pending".to_string(),
                },
            ],
        )
        .expect("保存提案");

        let pending = list_pending_change_proposals(&workspace, &node_id).expect("列出待处理提案");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, proposal.id);

        let (document, applied) =
            apply_report_changes(&workspace, &node_id, &proposal.id, &["c1".to_string()])
                .expect("应用选中修改");

        assert_eq!(document.revision, 2);
        assert!(document.content_md.contains("结论需谨慎解读"));
        assert!(document.content_md.contains("方法部分需要补充。"));
        assert_eq!(applied.status, "partially_applied");
        assert_eq!(applied.changes[0].review, "accepted");
        assert_eq!(applied.changes[1].review, "rejected");
        assert!(
            apply_report_changes(&workspace, &node_id, &proposal.id, &["c2".to_string()]).is_err()
        );
        assert!(list_pending_change_proposals(&workspace, &node_id)
            .expect("再次列出待处理提案")
            .is_empty());
    }
}

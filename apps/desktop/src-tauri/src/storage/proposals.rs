use std::collections::HashMap;

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::graph::{self, GraphSnapshot};
use super::{new_id, now_iso, open_project_db};
use crate::domain::WorkspaceInfo;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalNode {
    pub client_id: String,
    pub node_type: String,
    pub title: String,
    #[serde(default)]
    pub goal: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalEdge {
    pub source_id: String,
    pub target_id: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalView {
    pub id: String,
    pub anchor_node_id: String,
    pub base_graph_revision: i64,
    pub status: String,
    pub rationale: String,
    pub nodes: Vec<ProposalNode>,
    pub edges: Vec<ProposalEdge>,
}

pub fn insert_proposal(
    workspace: &WorkspaceInfo,
    anchor_node_id: &str,
    base_graph_revision: i64,
    rationale: &str,
    nodes: Vec<ProposalNode>,
    edges: Vec<ProposalEdge>,
) -> Result<ProposalView, String> {
    let connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let id = new_id("prop");
    let payload = serde_json::json!({
        "rationale": rationale,
        "nodes": nodes,
        "edges": edges,
    });
    let now = now_iso();

    connection
        .execute(
            "INSERT INTO proposal
                (id, project_id, anchor_node_id, base_graph_revision, status,
                 payload_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 'awaiting_approval', ?5, ?6, ?6)",
            params![
                id,
                workspace.project_id,
                anchor_node_id,
                base_graph_revision,
                serde_json::to_string(&payload)
                    .map_err(|error| format!("序列化节点提案失败：{error}"))?,
                now
            ],
        )
        .map_err(|error| format!("保存节点提案失败：{error}"))?;

    Ok(ProposalView {
        id,
        anchor_node_id: anchor_node_id.to_string(),
        base_graph_revision,
        status: "awaiting_approval".to_string(),
        rationale: rationale.to_string(),
        nodes,
        edges,
    })
}

pub fn apply_proposal(
    workspace: &WorkspaceInfo,
    proposal_id: &str,
    edited_nodes: Vec<ProposalNode>,
    edited_edges: Vec<ProposalEdge>,
) -> Result<GraphSnapshot, String> {
    let proposal = load_proposal(workspace, proposal_id)?;
    if proposal.status == "applied" {
        return graph::load_graph(workspace);
    }
    if proposal.status != "awaiting_approval" {
        return Err("该提案已不可应用。".to_string());
    }

    if edited_nodes.is_empty() || edited_nodes.len() > 6 {
        return Err("提案至少保留一个、最多六个新节点。".to_string());
    }
    let mut client_ids = std::collections::HashSet::new();
    for node in &edited_nodes {
        if !matches!(
            node.node_type.as_str(),
            "research" | "experiment" | "report"
        ) {
            return Err("提案节点类型必须是研究、实验或报告。".to_string());
        }
        if node.title.trim().is_empty() {
            return Err("提案节点名称不能为空。".to_string());
        }
        if !client_ids.insert(node.client_id.clone()) {
            return Err("提案中存在重复的候选节点标识。".to_string());
        }
    }

    let current_graph = graph::load_graph(workspace)?;
    if current_graph.graph_revision != proposal.base_graph_revision {
        return Err("科研图已在其他位置变化，请重新生成提案。".to_string());
    }

    let anchor = current_graph
        .nodes
        .iter()
        .find(|node| node.id == proposal.anchor_node_id)
        .ok_or_else(|| "提案关联的节点不存在。".to_string())?;
    let anchor_x = anchor.x;
    let anchor_y = anchor.y;

    let mut connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("无法开始提案事务：{error}"))?;

    let now = now_iso();
    let mut id_map: HashMap<String, String> = HashMap::new();

    for (index, node) in edited_nodes.iter().enumerate() {
        let node_id = new_id("node");
        let title = node.title.trim();
        transaction
            .execute(
                "INSERT INTO node
                    (id, project_id, type, title, goal, state, pos_x, pos_y,
                     revision, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'ready', ?6, ?7, 1, ?8, ?8)",
                params![
                    node_id,
                    workspace.project_id,
                    node.node_type,
                    title,
                    node.goal.trim(),
                    anchor_x + 340.0,
                    anchor_y + (index as f64) * 170.0,
                    now
                ],
            )
            .map_err(|error| format!("创建提案节点失败：{error}"))?;
        id_map.insert(node.client_id.clone(), node_id);
    }

    for edge in &edited_edges {
        if !matches!(edge.kind.as_str(), "decomposition" | "association") {
            return Err("提案关系类型无效。".to_string());
        }
        let source = id_map
            .get(&edge.source_id)
            .cloned()
            .or_else(|| {
                current_graph
                    .nodes
                    .iter()
                    .find(|node| node.id == edge.source_id)
                    .map(|node| node.id.clone())
            })
            .ok_or_else(|| "提案关系引用了不存在的来源节点。".to_string())?;
        let target = id_map
            .get(&edge.target_id)
            .cloned()
            .or_else(|| {
                current_graph
                    .nodes
                    .iter()
                    .find(|node| node.id == edge.target_id)
                    .map(|node| node.id.clone())
            })
            .ok_or_else(|| "提案关系引用了不存在的目标节点。".to_string())?;
        if source == target {
            return Err("提案关系不能连接节点自身。".to_string());
        }
        transaction
            .execute(
                "INSERT OR IGNORE INTO edge
                    (id, project_id, source_id, target_id, kind, label, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6)",
                params![
                    new_id("edge"),
                    workspace.project_id,
                    source,
                    target,
                    edge.kind,
                    now
                ],
            )
            .map_err(|error| format!("创建提案关系失败：{error}"))?;
    }

    transaction
        .execute(
            "UPDATE proposal SET status = 'applied', updated_at = ?1 WHERE id = ?2",
            params![now, proposal_id],
        )
        .map_err(|error| format!("更新提案状态失败：{error}"))?;

    bump_revision(&transaction, &workspace.project_id)?;
    transaction
        .commit()
        .map_err(|error| format!("提交提案失败：{error}"))?;

    graph::load_graph(workspace)
}

fn load_proposal(workspace: &WorkspaceInfo, proposal_id: &str) -> Result<ProposalView, String> {
    let connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let row: Option<(String, String, i64, String, String)> = connection
        .query_row(
            "SELECT anchor_node_id, status, base_graph_revision, payload_json, id
             FROM proposal WHERE project_id = ?1 AND id = ?2",
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
        .map_err(|error| format!("读取节点提案失败：{error}"))?;

    let Some((anchor_node_id, status, base_graph_revision, payload_json, id)) = row else {
        return Err("节点提案不存在。".to_string());
    };
    let payload: serde_json::Value =
        serde_json::from_str(&payload_json).unwrap_or(serde_json::Value::Null);
    let rationale = payload
        .get("rationale")
        .and_then(|item| item.as_str())
        .unwrap_or("")
        .to_string();
    let nodes: Vec<ProposalNode> = payload
        .get("nodes")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default();
    let edges: Vec<ProposalEdge> = payload
        .get("edges")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default();

    Ok(ProposalView {
        id,
        anchor_node_id,
        base_graph_revision,
        status,
        rationale,
        nodes,
        edges,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::open_workspace;

    #[test]
    fn applies_proposal_once_and_is_idempotent() {
        let directory = tempfile::tempdir().expect("创建临时目录");
        let workspace = open_workspace(directory.path()).expect("打开项目");
        let graph = graph::load_graph(&workspace).expect("读取图");
        let root_id = graph
            .nodes
            .iter()
            .find(|node| node.node_type == "root")
            .map(|node| node.id.clone())
            .expect("根节点");

        let proposal = insert_proposal(
            &workspace,
            &root_id,
            graph.graph_revision,
            "测试提案",
            vec![ProposalNode {
                client_id: "n1".to_string(),
                node_type: "research".to_string(),
                title: "新研究".to_string(),
                goal: "验证一个独立问题".to_string(),
            }],
            vec![ProposalEdge {
                source_id: root_id.clone(),
                target_id: "n1".to_string(),
                kind: "decomposition".to_string(),
            }],
        )
        .expect("保存提案");

        let nodes = vec![ProposalNode {
            client_id: "n1".to_string(),
            node_type: "research".to_string(),
            title: "新研究".to_string(),
            goal: "验证一个独立问题".to_string(),
        }];
        let edges = vec![ProposalEdge {
            source_id: root_id.clone(),
            target_id: "n1".to_string(),
            kind: "decomposition".to_string(),
        }];

        let applied = apply_proposal(&workspace, &proposal.id, nodes, edges).expect("应用提案");
        assert_eq!(applied.nodes.len(), 2);
        assert_eq!(applied.edges.len(), 1);

        let second = apply_proposal(&workspace, &proposal.id, Vec::new(), Vec::new())
            .expect("重复应用应幂等");
        assert_eq!(second.nodes.len(), 2);
        assert_eq!(second.edges.len(), 1);
    }
}

#[allow(clippy::items_after_test_module)]
fn bump_revision(transaction: &rusqlite::Transaction<'_>, project_id: &str) -> Result<(), String> {
    let key = format!("graph_revision:{project_id}");
    let current: Option<String> = transaction
        .query_row(
            "SELECT value FROM app_meta WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("读取图版本失败：{error}"))?;
    let next = current
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0)
        + 1;
    transaction
        .execute(
            "INSERT INTO app_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, next.to_string()],
        )
        .map_err(|error| format!("更新图版本失败：{error}"))?;
    Ok(())
}

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};

use super::{new_id, now_iso, open_project_db};
use crate::domain::WorkspaceInfo;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub title: String,
    pub goal: String,
    pub state: String,
    pub x: f64,
    pub y: f64,
    pub revision: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub kind: String,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Viewport {
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphSnapshot {
    pub graph_revision: i64,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub viewport: Option<Viewport>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateNodeInput {
    pub node_type: String,
    pub title: String,
    pub goal: Option<String>,
    pub x: f64,
    pub y: f64,
    pub parent_id: Option<String>,
    pub edge_kind: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateNodeInput {
    pub node_id: String,
    pub title: String,
    pub goal: Option<String>,
}

pub fn load_graph(workspace: &WorkspaceInfo) -> Result<GraphSnapshot, String> {
    let connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let graph_revision = read_revision(&connection, &workspace.project_id)?;

    let mut node_statement = connection
        .prepare(
            "SELECT id, type, title, goal, state, pos_x, pos_y, revision
             FROM node WHERE project_id = ?1 ORDER BY created_at, id",
        )
        .map_err(|error| format!("读取科研图节点失败：{error}"))?;

    let nodes = node_statement
        .query_map(params![workspace.project_id], |row| {
            Ok(GraphNode {
                id: row.get(0)?,
                node_type: row.get(1)?,
                title: row.get(2)?,
                goal: row.get(3)?,
                state: row.get(4)?,
                x: row.get(5)?,
                y: row.get(6)?,
                revision: row.get(7)?,
            })
        })
        .map_err(|error| format!("读取科研图节点失败：{error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("读取科研图节点失败：{error}"))?;

    let mut edge_statement = connection
        .prepare(
            "SELECT id, source_id, target_id, kind, label
             FROM edge WHERE project_id = ?1 ORDER BY created_at, id",
        )
        .map_err(|error| format!("读取科研图关系失败：{error}"))?;

    let edges = edge_statement
        .query_map(params![workspace.project_id], |row| {
            Ok(GraphEdge {
                id: row.get(0)?,
                source: row.get(1)?,
                target: row.get(2)?,
                kind: row.get(3)?,
                label: row.get(4)?,
            })
        })
        .map_err(|error| format!("读取科研图关系失败：{error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("读取科研图关系失败：{error}"))?;

    Ok(GraphSnapshot {
        graph_revision,
        nodes,
        edges,
        viewport: load_viewport(&connection, &workspace.project_id)?,
    })
}

pub fn create_node(
    workspace: &WorkspaceInfo,
    input: CreateNodeInput,
) -> Result<GraphSnapshot, String> {
    let node_type = input.node_type.trim();
    if !matches!(node_type, "research" | "experiment" | "report") {
        return Err("只能创建研究、实验或报告节点。".to_string());
    }

    let title = input.title.trim();
    if title.is_empty() {
        return Err("节点名称不能为空。".to_string());
    }
    if title.chars().count() > 120 {
        return Err("节点名称不能超过 120 个字符。".to_string());
    }

    let goal = input.goal.unwrap_or_default().trim().to_string();
    let edge_kind = input.edge_kind.unwrap_or_else(|| "association".to_string());
    if !matches!(edge_kind.as_str(), "decomposition" | "association") {
        return Err("关系类型无效。".to_string());
    }

    let x = clamp_coordinate(input.x);
    let y = clamp_coordinate(input.y);
    let mut connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("无法开始科研图事务：{error}"))?;

    if let Some(parent_id) = input.parent_id.as_deref() {
        ensure_node_exists(&transaction, &workspace.project_id, parent_id)?;
    }

    let node_id = new_id("node");
    let now = now_iso();
    transaction
        .execute(
            "INSERT INTO node (id, project_id, type, title, goal, state, pos_x, pos_y, revision, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'ready', ?6, ?7, 1, ?8, ?8)",
            params![
                node_id,
                workspace.project_id,
                node_type,
                title,
                goal,
                x,
                y,
                now
            ],
        )
        .map_err(|error| format!("创建节点失败：{error}"))?;

    if let Some(parent_id) = input.parent_id.as_deref() {
        insert_edge(
            &transaction,
            &workspace.project_id,
            parent_id,
            &node_id,
            &edge_kind,
            &now,
        )?;
    }

    bump_revision(&transaction, &workspace.project_id)?;
    transaction
        .commit()
        .map_err(|error| format!("提交科研图事务失败：{error}"))?;

    load_graph(workspace)
}

pub fn move_node(workspace: &WorkspaceInfo, node_id: &str, x: f64, y: f64) -> Result<(), String> {
    let connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let now = now_iso();
    let changed = connection
        .execute(
            "UPDATE node SET pos_x = ?1, pos_y = ?2, updated_at = ?3
             WHERE project_id = ?4 AND id = ?5",
            params![
                clamp_coordinate(x),
                clamp_coordinate(y),
                now,
                workspace.project_id,
                node_id
            ],
        )
        .map_err(|error| format!("保存节点位置失败：{error}"))?;

    if changed == 0 {
        return Err("节点不存在或不属于当前项目。".to_string());
    }
    Ok(())
}

pub fn update_node(
    workspace: &WorkspaceInfo,
    input: UpdateNodeInput,
) -> Result<GraphSnapshot, String> {
    let title = input.title.trim();
    if title.is_empty() {
        return Err("节点名称不能为空。".to_string());
    }
    if title.chars().count() > 120 {
        return Err("节点名称不能超过 120 个字符。".to_string());
    }
    let goal = input.goal.unwrap_or_default().trim().to_string();

    let mut connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("无法开始科研图事务：{error}"))?;

    let existing: Option<(String, String)> = transaction
        .query_row(
            "SELECT type, title FROM node WHERE project_id = ?1 AND id = ?2",
            params![workspace.project_id, input.node_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| format!("读取节点失败：{error}"))?;

    let Some((node_type, existing_title)) = existing else {
        return Err("节点不存在或不属于当前项目。".to_string());
    };
    if node_type == "root" && title != existing_title {
        return Err("总课题名称与项目名绑定，暂不支持在此修改名称。".to_string());
    }

    transaction
        .execute(
            "UPDATE node SET title = ?1, goal = ?2, updated_at = ?3
             WHERE project_id = ?4 AND id = ?5",
            params![title, goal, now_iso(), workspace.project_id, input.node_id],
        )
        .map_err(|error| format!("更新节点失败：{error}"))?;

    bump_revision(&transaction, &workspace.project_id)?;
    transaction
        .commit()
        .map_err(|error| format!("提交科研图事务失败：{error}"))?;

    load_graph(workspace)
}

pub fn delete_node(workspace: &WorkspaceInfo, node_id: &str) -> Result<GraphSnapshot, String> {
    let mut connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("无法开始科研图事务：{error}"))?;

    let node_type: Option<String> = transaction
        .query_row(
            "SELECT type FROM node WHERE project_id = ?1 AND id = ?2",
            params![workspace.project_id, node_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("读取节点失败：{error}"))?;

    let Some(node_type) = node_type else {
        return Err("节点不存在或不属于当前项目。".to_string());
    };
    if node_type == "root" {
        return Err("总课题根节点不能删除。".to_string());
    }

    transaction
        .execute(
            "DELETE FROM node WHERE project_id = ?1 AND id = ?2",
            params![workspace.project_id, node_id],
        )
        .map_err(|error| format!("删除节点失败：{error}"))?;

    bump_revision(&transaction, &workspace.project_id)?;
    transaction
        .commit()
        .map_err(|error| format!("提交科研图事务失败：{error}"))?;

    load_graph(workspace)
}

pub fn delete_edge(workspace: &WorkspaceInfo, edge_id: &str) -> Result<GraphSnapshot, String> {
    let mut connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("无法开始科研图事务：{error}"))?;

    let changed = transaction
        .execute(
            "DELETE FROM edge WHERE project_id = ?1 AND id = ?2",
            params![workspace.project_id, edge_id],
        )
        .map_err(|error| format!("删除关系失败：{error}"))?;
    if changed == 0 {
        return Err("关系不存在或不属于当前项目。".to_string());
    }

    bump_revision(&transaction, &workspace.project_id)?;
    transaction
        .commit()
        .map_err(|error| format!("提交科研图事务失败：{error}"))?;

    load_graph(workspace)
}

pub fn save_viewport(workspace: &WorkspaceInfo, x: f64, y: f64, zoom: f64) -> Result<(), String> {
    let viewport = Viewport {
        x: clamp_coordinate(x),
        y: clamp_coordinate(y),
        zoom: if zoom.is_finite() {
            zoom.clamp(0.1, 3.0)
        } else {
            1.0
        },
    };
    let value =
        serde_json::to_string(&viewport).map_err(|error| format!("序列化画布视口失败：{error}"))?;
    let connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    connection
        .execute(
            "INSERT INTO app_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![viewport_key(&workspace.project_id), value],
        )
        .map_err(|error| format!("保存画布视口失败：{error}"))?;
    Ok(())
}

fn load_viewport(connection: &Connection, project_id: &str) -> Result<Option<Viewport>, String> {
    let value: Option<String> = connection
        .query_row(
            "SELECT value FROM app_meta WHERE key = ?1",
            params![viewport_key(project_id)],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("读取画布视口失败：{error}"))?;
    Ok(value.and_then(|value| serde_json::from_str(&value).ok()))
}

fn viewport_key(project_id: &str) -> String {
    format!("viewport:{project_id}")
}

pub fn create_edge(
    workspace: &WorkspaceInfo,
    source: &str,
    target: &str,
    kind: &str,
) -> Result<GraphSnapshot, String> {
    if source == target {
        return Err("不能把节点连接到自身。".to_string());
    }
    if !matches!(kind, "decomposition" | "association") {
        return Err("关系类型无效。".to_string());
    }

    let mut connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("无法开始科研图事务：{error}"))?;

    ensure_node_exists(&transaction, &workspace.project_id, source)?;
    ensure_node_exists(&transaction, &workspace.project_id, target)?;

    let duplicate: Option<String> = transaction
        .query_row(
            "SELECT id FROM edge
             WHERE project_id = ?1 AND source_id = ?2 AND target_id = ?3 AND kind = ?4",
            params![workspace.project_id, source, target, kind],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("检查已有关系失败：{error}"))?;
    if duplicate.is_some() {
        return Err("这两个节点之间已经存在同类型关系。".to_string());
    }

    if kind == "decomposition"
        && decomposition_reaches(&transaction, &workspace.project_id, target, source)?
    {
        return Err("分解关系不能形成循环。".to_string());
    }

    let now = now_iso();
    insert_edge(
        &transaction,
        &workspace.project_id,
        source,
        target,
        kind,
        &now,
    )?;
    bump_revision(&transaction, &workspace.project_id)?;
    transaction
        .commit()
        .map_err(|error| format!("提交科研图事务失败：{error}"))?;

    load_graph(workspace)
}

fn insert_edge(
    transaction: &Transaction<'_>,
    project_id: &str,
    source: &str,
    target: &str,
    kind: &str,
    now: &str,
) -> Result<(), String> {
    transaction
        .execute(
            "INSERT INTO edge (id, project_id, source_id, target_id, kind, label, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6)",
            params![new_id("edge"), project_id, source, target, kind, now],
        )
        .map_err(|error| format!("创建科研关系失败：{error}"))?;
    Ok(())
}

fn ensure_node_exists(
    transaction: &Transaction<'_>,
    project_id: &str,
    node_id: &str,
) -> Result<(), String> {
    let found: Option<String> = transaction
        .query_row(
            "SELECT id FROM node WHERE project_id = ?1 AND id = ?2",
            params![project_id, node_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("检查节点失败：{error}"))?;

    if found.is_none() {
        return Err("节点不存在或不属于当前项目。".to_string());
    }
    Ok(())
}

fn decomposition_reaches(
    transaction: &Transaction<'_>,
    project_id: &str,
    from: &str,
    to: &str,
) -> Result<bool, String> {
    let found: Option<i64> = transaction
        .query_row(
            "WITH RECURSIVE reach(id) AS (
                 SELECT ?1
                 UNION
                 SELECT edge.target_id
                 FROM edge JOIN reach ON edge.source_id = reach.id
                 WHERE edge.project_id = ?2 AND edge.kind = 'decomposition'
             )
             SELECT 1 FROM reach WHERE id = ?3 LIMIT 1",
            params![from, project_id, to],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("检查分解关系循环失败：{error}"))?;
    Ok(found.is_some())
}

fn read_revision(connection: &Connection, project_id: &str) -> Result<i64, String> {
    let key = format!("graph_revision:{project_id}");
    let value: Option<String> = connection
        .query_row(
            "SELECT value FROM app_meta WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("读取图版本失败：{error}"))?;
    Ok(value.and_then(|value| value.parse().ok()).unwrap_or(0))
}

fn bump_revision(transaction: &Transaction<'_>, project_id: &str) -> Result<(), String> {
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

fn clamp_coordinate(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(-100_000.0, 100_000.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::open_workspace;

    fn create(
        workspace: &WorkspaceInfo,
        node_type: &str,
        title: &str,
        parent_id: Option<String>,
        edge_kind: Option<&str>,
    ) -> GraphSnapshot {
        create_node(
            workspace,
            CreateNodeInput {
                node_type: node_type.to_string(),
                title: title.to_string(),
                goal: Some("测试目标".to_string()),
                x: 10.0,
                y: 20.0,
                parent_id,
                edge_kind: edge_kind.map(str::to_string),
            },
        )
        .expect("创建节点")
    }

    fn id_of(snapshot: &GraphSnapshot, title: &str) -> String {
        snapshot
            .nodes
            .iter()
            .find(|node| node.title == title)
            .map(|node| node.id.clone())
            .expect("找到节点")
    }

    #[test]
    fn creates_nodes_edges_and_rejects_decomposition_cycles() {
        let directory = tempfile::tempdir().expect("创建临时目录");
        let workspace = open_workspace(directory.path()).expect("打开项目");

        let snapshot = create(&workspace, "research", "研究 A", None, None);
        let a = id_of(&snapshot, "研究 A");

        let snapshot = create(
            &workspace,
            "experiment",
            "实验 B",
            Some(a.clone()),
            Some("decomposition"),
        );
        let b = id_of(&snapshot, "实验 B");
        assert_eq!(snapshot.edges.len(), 1);

        assert!(create_edge(&workspace, &a, &b, "decomposition").is_err());
        assert!(create_edge(&workspace, &b, &a, "decomposition").is_err());

        let snapshot = create_edge(&workspace, &b, &a, "association").expect("建立关联");
        assert_eq!(snapshot.edges.len(), 2);
        assert!(snapshot.graph_revision >= 3);

        move_node(&workspace, &a, 120.0, 240.0).expect("保存位置");
        let loaded = load_graph(&workspace).expect("读取科研图");
        let moved = loaded
            .nodes
            .iter()
            .find(|node| node.id == a)
            .expect("找到移动后的节点");
        assert_eq!(moved.x, 120.0);
        assert_eq!(moved.y, 240.0);

        let updated = update_node(
            &workspace,
            UpdateNodeInput {
                node_id: a.clone(),
                title: "研究 A（修订）".to_string(),
                goal: Some("更新后的目标".to_string()),
            },
        )
        .expect("更新节点");
        let node = updated
            .nodes
            .iter()
            .find(|node| node.id == a)
            .expect("找到更新后的节点");
        assert_eq!(node.title, "研究 A（修订）");
        assert_eq!(node.goal, "更新后的目标");

        let root_id = updated
            .nodes
            .iter()
            .find(|node| node.node_type == "root")
            .map(|node| node.id.clone())
            .expect("根节点");
        assert!(delete_node(&workspace, &root_id).is_err());

        save_viewport(&workspace, 100.0, 50.0, 0.8).expect("保存视口");
        let restored = load_graph(&workspace).expect("读取视口");
        let viewport = restored.viewport.expect("视口存在");
        assert_eq!(viewport.x, 100.0);
        assert_eq!(viewport.y, 50.0);
        assert!((viewport.zoom - 0.8).abs() < f64::EPSILON);

        let edge_id = restored.edges[0].id.clone();
        let after_edge_delete = delete_edge(&workspace, &edge_id).expect("删除关系");
        assert_eq!(after_edge_delete.edges.len(), 1);

        let after_node_delete = delete_node(&workspace, &b).expect("删除节点");
        assert!(!after_node_delete.nodes.iter().any(|node| node.id == b));
        assert!(after_node_delete.edges.is_empty());
    }
}

use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use serde_json::Value;

use super::{new_id, now_iso, open_project_db};
use crate::domain::WorkspaceInfo;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentRun {
    pub id: String,
    pub node_id: String,
    pub asset_id: Option<String>,
    pub goal: String,
    pub state: String,
    pub outputs: Value,
    pub error: Option<String>,
    pub created_at: String,
    pub finished_at: Option<String>,
}

pub struct NewExperimentRun<'a> {
    pub node_id: &'a str,
    pub asset_id: Option<&'a str>,
    pub input_path: Option<&'a str>,
    pub goal: &'a str,
    pub parameters: &'a Value,
    pub state: &'a str,
    pub outputs: &'a Value,
    pub error: Option<&'a str>,
}

pub fn ensure_experiment_node(workspace: &WorkspaceInfo, node_id: &str) -> Result<(), String> {
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
        Some("experiment") => Ok(()),
        Some(_) => Err("只有实验节点可以运行实验分析。".to_string()),
        None => Err("节点不存在或不属于当前项目。".to_string()),
    }
}

pub fn save_run(
    workspace: &WorkspaceInfo,
    run: NewExperimentRun<'_>,
) -> Result<ExperimentRun, String> {
    let connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let now = now_iso();
    let saved = ExperimentRun {
        id: new_id("run"),
        node_id: run.node_id.to_string(),
        asset_id: run.asset_id.map(str::to_string),
        goal: run.goal.to_string(),
        state: run.state.to_string(),
        outputs: run.outputs.clone(),
        error: run.error.map(str::to_string),
        created_at: now.clone(),
        finished_at: Some(now),
    };

    connection
        .execute(
            "INSERT INTO experiment_run
                (id, project_id, node_id, asset_id, input_path, goal,
                 parameters_json, state, outputs_json, error, created_at, finished_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                saved.id,
                workspace.project_id,
                saved.node_id,
                saved.asset_id,
                run.input_path,
                saved.goal,
                serde_json::to_string(run.parameters)
                    .map_err(|error| format!("序列化实验参数失败：{error}"))?,
                saved.state,
                serde_json::to_string(&saved.outputs)
                    .map_err(|error| format!("序列化实验结果失败：{error}"))?,
                saved.error,
                saved.created_at,
                saved.finished_at
            ],
        )
        .map_err(|error| format!("保存实验运行失败：{error}"))?;

    Ok(saved)
}

pub fn list_runs(workspace: &WorkspaceInfo, node_id: &str) -> Result<Vec<ExperimentRun>, String> {
    let connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let mut statement = connection
        .prepare(
            "SELECT id, node_id, asset_id, goal, state, outputs_json, error,
                    created_at, finished_at
             FROM experiment_run
             WHERE project_id = ?1 AND node_id = ?2
             ORDER BY created_at DESC, id DESC
             LIMIT 50",
        )
        .map_err(|error| format!("读取实验运行失败：{error}"))?;

    let rows = statement
        .query_map(params![workspace.project_id, node_id], |row| {
            let outputs_json: String = row.get(5)?;
            Ok(ExperimentRun {
                id: row.get(0)?,
                node_id: row.get(1)?,
                asset_id: row.get(2)?,
                goal: row.get(3)?,
                state: row.get(4)?,
                outputs: serde_json::from_str(&outputs_json).unwrap_or(Value::Null),
                error: row.get(6)?,
                created_at: row.get(7)?,
                finished_at: row.get(8)?,
            })
        })
        .map_err(|error| format!("读取实验运行失败：{error}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("读取实验运行失败：{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::graph::{create_node, CreateNodeInput};
    use crate::storage::open_workspace;

    #[test]
    fn saves_and_lists_multiple_runs() {
        let directory = tempfile::tempdir().expect("创建临时目录");
        let workspace = open_workspace(directory.path()).expect("打开项目");
        let snapshot = create_node(
            &workspace,
            CreateNodeInput {
                node_type: "experiment".to_string(),
                title: "实验".to_string(),
                goal: Some("验证".to_string()),
                x: 10.0,
                y: 10.0,
                parent_id: None,
                edge_kind: None,
            },
        )
        .expect("创建实验节点");
        let node_id = snapshot
            .nodes
            .iter()
            .find(|node| node.title == "实验")
            .map(|node| node.id.clone())
            .expect("找到实验节点");

        let outputs = serde_json::json!({"ok": true, "summary": {"rows": 3}});
        let first = save_run(
            &workspace,
            NewExperimentRun {
                node_id: &node_id,
                asset_id: None,
                input_path: Some("data.csv"),
                goal: "第一轮",
                parameters: &serde_json::json!({}),
                state: "completed",
                outputs: &outputs,
                error: None,
            },
        )
        .expect("保存第一轮");
        let second = save_run(
            &workspace,
            NewExperimentRun {
                node_id: &node_id,
                asset_id: None,
                input_path: Some("data.csv"),
                goal: "第二轮",
                parameters: &serde_json::json!({"filter": "x"}),
                state: "completed",
                outputs: &outputs,
                error: None,
            },
        )
        .expect("保存第二轮");

        let runs = list_runs(&workspace, &node_id).expect("列出运行");
        assert_eq!(runs.len(), 2);
        assert_ne!(first.id, second.id);
        assert_eq!(runs[0].id, second.id);
    }
}

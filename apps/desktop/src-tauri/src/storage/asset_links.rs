use std::path::Path;

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::{assets, now_iso, open_project_db};
use crate::domain::WorkspaceInfo;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkedAsset {
    pub id: String,
    pub name: String,
    pub relative_path: String,
    pub kind: String,
    pub status: String,
    pub has_text: bool,
}

pub fn link_asset(workspace: &WorkspaceInfo, node_id: &str, asset_id: &str) -> Result<(), String> {
    ensure_owned(workspace, "node", node_id)?;
    ensure_owned(workspace, "asset", asset_id)?;
    let connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    connection
        .execute(
            "INSERT OR IGNORE INTO asset_node_link (project_id, asset_id, node_id, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![workspace.project_id, asset_id, node_id, now_iso()],
        )
        .map_err(|error| format!("引用资产失败：{error}"))?;
    Ok(())
}

pub fn unlink_asset(
    workspace: &WorkspaceInfo,
    node_id: &str,
    asset_id: &str,
) -> Result<(), String> {
    let connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    connection
        .execute(
            "DELETE FROM asset_node_link
             WHERE project_id = ?1 AND node_id = ?2 AND asset_id = ?3",
            params![workspace.project_id, node_id, asset_id],
        )
        .map_err(|error| format!("取消资产引用失败：{error}"))?;
    Ok(())
}

pub fn list_linked_assets(
    workspace: &WorkspaceInfo,
    node_id: &str,
) -> Result<Vec<LinkedAsset>, String> {
    let connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let mut statement = connection
        .prepare(
            "SELECT a.id, a.path, a.kind, a.status,
                    EXISTS(SELECT 1 FROM asset_fts f WHERE f.asset_id = a.id
                           AND length(trim(f.content)) > 0)
             FROM asset_node_link l
             JOIN asset a ON a.id = l.asset_id
             WHERE l.project_id = ?1 AND l.node_id = ?2
             ORDER BY l.created_at DESC, a.path COLLATE NOCASE",
        )
        .map_err(|error| format!("读取节点引用资产失败：{error}"))?;
    let rows = statement
        .query_map(params![workspace.project_id, node_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })
        .map_err(|error| format!("读取节点引用资产失败：{error}"))?;

    let root = Path::new(&workspace.workspace_path);
    let mut items = Vec::new();
    for row in rows {
        let (id, relative_path, kind, status, has_text) =
            row.map_err(|error| format!("读取节点引用资产失败：{error}"))?;
        let absolute = root.join(&relative_path);
        let effective_status = if absolute.is_file() {
            status
        } else {
            "missing".to_string()
        };
        let name = Path::new(&relative_path)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(&relative_path)
            .to_string();
        items.push(LinkedAsset {
            id,
            name,
            relative_path,
            kind,
            status: effective_status,
            has_text: has_text != 0,
        });
    }
    Ok(items)
}

pub fn linked_excerpts(
    workspace: &WorkspaceInfo,
    node_id: &str,
    max_items: usize,
    max_chars: usize,
) -> Result<Vec<(String, String)>, String> {
    let assets = list_linked_assets(workspace, node_id)?;
    let mut excerpts = Vec::new();
    for asset in assets {
        if excerpts.len() >= max_items {
            break;
        }
        if !asset.has_text {
            excerpts.push((asset.name.clone(), "（该资产暂无可提取的正文）".to_string()));
            continue;
        }
        if let Some(text) = assets::get_asset_text(workspace, &asset.id, Some(max_chars))? {
            if !text.trim().is_empty() {
                excerpts.push((asset.name, text));
            }
        }
    }
    Ok(excerpts)
}

fn ensure_owned(workspace: &WorkspaceInfo, table: &str, id: &str) -> Result<(), String> {
    let connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let found: Option<i64> = connection
        .query_row(
            &format!("SELECT 1 FROM {table} WHERE project_id = ?1 AND id = ?2"),
            params![workspace.project_id, id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("读取节点或资产失败：{error}"))?;
    if found.is_none() {
        return Err("节点或资产不存在，或不属于当前项目。".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::assets::{list_assets, scan_workspace};
    use crate::storage::graph::{create_node, CreateNodeInput};
    use crate::storage::open_workspace;
    use std::fs;

    #[test]
    fn links_and_unlinks_assets_to_nodes() {
        let directory = tempfile::tempdir().expect("创建临时目录");
        fs::write(directory.path().join("paper.md"), "城市绿地证据摘录。").expect("写入资料");
        let workspace = open_workspace(directory.path()).expect("打开项目");
        scan_workspace(&workspace).expect("扫描资产");
        let asset = list_assets(&workspace, None, None)
            .expect("列出资产")
            .into_iter()
            .next()
            .expect("资产存在");

        let snapshot = create_node(
            &workspace,
            CreateNodeInput {
                node_type: "research".to_string(),
                title: "引用测试".to_string(),
                goal: None,
                x: 0.0,
                y: 0.0,
                parent_id: None,
                edge_kind: None,
            },
        )
        .expect("创建节点");
        let node_id = snapshot.nodes[0].id.clone();

        link_asset(&workspace, &node_id, &asset.id).expect("引用资产");
        let linked = list_linked_assets(&workspace, &node_id).expect("列出引用");
        assert_eq!(linked.len(), 1);
        assert_eq!(linked[0].id, asset.id);
        assert!(linked[0].has_text);

        let excerpts = linked_excerpts(&workspace, &node_id, 3, 100).expect("读取摘录");
        assert_eq!(excerpts.len(), 1);
        assert!(excerpts[0].1.contains("城市绿地"));

        unlink_asset(&workspace, &node_id, &asset.id).expect("取消引用");
        assert!(list_linked_assets(&workspace, &node_id)
            .expect("再次列出")
            .is_empty());
    }
}

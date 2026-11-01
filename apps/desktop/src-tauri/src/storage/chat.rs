use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::{new_id, now_iso, open_project_db};
use crate::domain::WorkspaceInfo;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    pub role: String,
    pub body: String,
    pub meta: Option<serde_json::Value>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeChatContext {
    pub node_type: String,
    pub title: String,
    pub goal: String,
}

pub fn node_context(workspace: &WorkspaceInfo, node_id: &str) -> Result<NodeChatContext, String> {
    let connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let context = connection
        .query_row(
            "SELECT type, title, goal FROM node WHERE project_id = ?1 AND id = ?2",
            params![workspace.project_id, node_id],
            |row| {
                Ok(NodeChatContext {
                    node_type: row.get(0)?,
                    title: row.get(1)?,
                    goal: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(|error| format!("读取节点上下文失败：{error}"))?;

    context.ok_or_else(|| "节点不存在或不属于当前项目。".to_string())
}

pub fn load_messages(
    workspace: &WorkspaceInfo,
    node_id: &str,
    limit: usize,
) -> Result<Vec<ChatMessage>, String> {
    let connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let limit = limit.clamp(1, 50) as i64;
    let mut statement = connection
        .prepare(
            "SELECT id, role, body, meta_json, created_at
             FROM conversation
             WHERE project_id = ?1 AND node_id = ?2
             ORDER BY created_at DESC, id DESC
             LIMIT ?3",
        )
        .map_err(|error| format!("读取对话失败：{error}"))?;

    let rows = statement
        .query_map(params![workspace.project_id, node_id, limit], |row| {
            Ok(ChatMessage {
                id: row.get(0)?,
                role: row.get(1)?,
                body: row.get(2)?,
                meta: row
                    .get::<_, Option<String>>(3)?
                    .and_then(|value| serde_json::from_str(&value).ok()),
                created_at: row.get(4)?,
            })
        })
        .map_err(|error| format!("读取对话失败：{error}"))?;

    let mut messages = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("读取对话失败：{error}"))?;
    messages.reverse();
    Ok(messages)
}

pub fn append_message(
    workspace: &WorkspaceInfo,
    node_id: &str,
    role: &str,
    body: &str,
) -> Result<ChatMessage, String> {
    append_message_with_meta(workspace, node_id, role, body, None)
}

pub fn append_message_with_meta(
    workspace: &WorkspaceInfo,
    node_id: &str,
    role: &str,
    body: &str,
    meta: Option<serde_json::Value>,
) -> Result<ChatMessage, String> {
    if !matches!(role, "user" | "assistant" | "system") {
        return Err("对话角色无效。".to_string());
    }
    let text = body.trim();
    if text.is_empty() {
        return Err("消息内容不能为空。".to_string());
    }

    let connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let message = ChatMessage {
        id: new_id("msg"),
        role: role.to_string(),
        body: text.to_string(),
        meta: meta.clone(),
        created_at: now_iso(),
    };
    let meta_json = meta
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| format!("序列化对话元数据失败：{error}"))?;

    connection
        .execute(
            "INSERT INTO conversation (id, project_id, node_id, role, body, meta_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                message.id,
                workspace.project_id,
                node_id,
                message.role,
                message.body,
                meta_json,
                message.created_at
            ],
        )
        .map_err(|error| format!("保存对话失败：{error}"))?;

    Ok(message)
}

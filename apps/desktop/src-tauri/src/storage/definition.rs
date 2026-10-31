use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::{now_iso, open_project_db};
use crate::domain::WorkspaceInfo;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchDefinitionContent {
    #[serde(default)]
    pub question: Option<String>,
    #[serde(default)]
    pub goal: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub approach: Option<String>,
    #[serde(default)]
    pub expected_outputs: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchDefinition {
    pub content: ResearchDefinitionContent,
    pub confirmed_fields: Vec<String>,
    pub revision: i64,
    pub updated_at: Option<String>,
}

pub fn get_definition(workspace: &WorkspaceInfo) -> Result<ResearchDefinition, String> {
    let connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let row: Option<(String, String, i64, String)> = connection
        .query_row(
            "SELECT content_json, confirmed_fields, revision, updated_at
             FROM research_definition WHERE project_id = ?1",
            params![workspace.project_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(|error| format!("读取研究定义失败：{error}"))?;

    let Some((content_json, confirmed_json, revision, updated_at)) = row else {
        return Ok(ResearchDefinition {
            content: ResearchDefinitionContent::default(),
            confirmed_fields: Vec::new(),
            revision: 0,
            updated_at: None,
        });
    };

    let content: ResearchDefinitionContent =
        serde_json::from_str(&content_json).unwrap_or_default();
    let confirmed_fields: Vec<String> = serde_json::from_str(&confirmed_json).unwrap_or_default();
    Ok(ResearchDefinition {
        content,
        confirmed_fields,
        revision,
        updated_at: Some(updated_at),
    })
}

pub fn save_definition(
    workspace: &WorkspaceInfo,
    content: ResearchDefinitionContent,
    confirmed_fields: Vec<String>,
    base_revision: i64,
) -> Result<ResearchDefinition, String> {
    let mut connection = open_project_db(workspace).map_err(|error| error.to_string())?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("无法开始研究定义事务：{error}"))?;

    let current: Option<i64> = transaction
        .query_row(
            "SELECT revision FROM research_definition WHERE project_id = ?1",
            params![workspace.project_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("读取研究定义版本失败：{error}"))?;
    let current_revision = current.unwrap_or(0);

    if base_revision != current_revision {
        return Err("研究定义已在其他位置更新，请刷新后重试。".to_string());
    }

    let next_revision = current_revision + 1;
    let now = now_iso();
    let content_json =
        serde_json::to_string(&content).map_err(|error| format!("序列化研究定义失败：{error}"))?;
    let confirmed_json = serde_json::to_string(&confirmed_fields)
        .map_err(|error| format!("序列化确认字段失败：{error}"))?;

    transaction
        .execute(
            "INSERT INTO research_definition
                (project_id, content_json, confirmed_fields, revision, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(project_id) DO UPDATE SET
                 content_json = excluded.content_json,
                 confirmed_fields = excluded.confirmed_fields,
                 revision = excluded.revision,
                 updated_at = excluded.updated_at",
            params![
                workspace.project_id,
                content_json,
                confirmed_json,
                next_revision,
                now
            ],
        )
        .map_err(|error| format!("保存研究定义失败：{error}"))?;

    transaction
        .commit()
        .map_err(|error| format!("提交研究定义失败：{error}"))?;

    get_definition(workspace)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::open_workspace;

    #[test]
    fn saves_and_versions_research_definition() {
        let directory = tempfile::tempdir().expect("创建临时目录");
        let workspace = open_workspace(directory.path()).expect("打开项目");

        let empty = get_definition(&workspace).expect("读取空定义");
        assert_eq!(empty.revision, 0);

        let saved = save_definition(
            &workspace,
            ResearchDefinitionContent {
                question: Some("城市绿地是否降低局地温度？".to_string()),
                goal: Some("比较不同绿地覆盖条件下的温度差异。".to_string()),
                scope: Some("中国城市".to_string()),
                approach: None,
                expected_outputs: vec!["阶段报告".to_string()],
            },
            vec!["question".to_string(), "goal".to_string()],
            0,
        )
        .expect("保存定义");
        assert_eq!(saved.revision, 1);
        assert_eq!(
            saved.content.question.as_deref(),
            Some("城市绿地是否降低局地温度？")
        );

        assert!(save_definition(
            &workspace,
            ResearchDefinitionContent::default(),
            Vec::new(),
            0
        )
        .is_err());

        let updated = save_definition(
            &workspace,
            ResearchDefinitionContent::default(),
            Vec::new(),
            1,
        )
        .expect("更新定义");
        assert_eq!(updated.revision, 2);
    }
}

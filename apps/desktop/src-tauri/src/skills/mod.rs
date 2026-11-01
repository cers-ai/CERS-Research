use std::fs;
use std::path::PathBuf;

use serde::Serialize;

use crate::settings::{self, GlobalSettings};

const SEED_SKILL: &str = "---\nname: 学术趋势研究\nversion: \"1.0\"\ndescription: 对一个研究主题做有来源的最新动态与趋势分析\npermissions: academic_search, news_search, mcp_tools\ninputs: 研究主题、时间范围、关注维度\noutputs: 带来源与证据等级的趋势报告\n---\n\n# 学术趋势研究\n\n按以下顺序执行，不要跳过证据核验：\n\n1. **界定范围**：明确主题、时间窗口（默认最近 24 个月）、地域与关注维度；说明哪些问题本轮不覆盖。\n2. **学术检索**：用 `cers_academic_search` 至少检索 2 组不同关键词，优先近三年文献；记录题录、摘要与覆盖级别。\n3. **动态检索**：用 `cers_news_search` 获取近期新闻与监管动态；新闻只作为线索，不当作已证实的研究结论。\n4. **原文核验**：对关键监管文件、企业公告或新闻，用 MCP 的 `fetch_web_page` 读取原文；引用时给出链接与发布日期。\n5. **证据分级**：区分监管文件、同行评议研究、行业报告、厂商观点与媒体报道；标注“已验证 / 待核实 / 仅为线索”。\n6. **输出结构**：摘要、最新动态（按时间）、主要模式、发展趋势、证据表、风险与空白、参考文献。\n7. **诚实边界**：没有全文时写明“仅摘要/仅题录”；不得编造数据、市场份额、法规条款或页码。\n";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub source: String,
    pub permissions: Vec<String>,
    pub inputs: String,
    pub outputs: String,
    pub enabled: bool,
    pub chars: usize,
}

pub fn skills_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    let directory = app
        .path()
        .app_config_dir()
        .map_err(|error| format!("无法定位应用配置目录：{error}"))?
        .join("skills");
    if !directory.exists() {
        fs::create_dir_all(&directory).map_err(|error| format!("创建技能目录失败：{error}"))?;
        fs::write(directory.join("academic-trend-research.md"), SEED_SKILL)
            .map_err(|error| format!("写入示例技能失败：{error}"))?;
    }
    Ok(directory)
}

pub fn list_skills(
    app: &tauri::AppHandle,
    settings: &GlobalSettings,
) -> Result<Vec<SkillInfo>, String> {
    let directory = skills_dir(app)?;
    let mut skills = Vec::new();
    let entries = fs::read_dir(&directory).map_err(|error| format!("读取技能目录失败：{error}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|value| value.to_str()) != Some("md") {
            continue;
        }
        let id = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_string();
        if id.is_empty() {
            continue;
        }
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(_) => continue,
        };
        let (meta, body) = split_frontmatter(&text);
        let name = meta_value(&meta, "name").unwrap_or_else(|| id.clone());
        let description = meta_value(&meta, "description").unwrap_or_default();
        let version = meta_value(&meta, "version").unwrap_or_else(|| "0.1".to_string());
        let permissions = meta_value(&meta, "permissions")
            .map(|value| {
                value
                    .split([',', '，'])
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let inputs = meta_value(&meta, "inputs").unwrap_or_default();
        let outputs = meta_value(&meta, "outputs").unwrap_or_default();
        skills.push(SkillInfo {
            id: id.clone(),
            name,
            description,
            version,
            source: path.display().to_string(),
            permissions,
            inputs,
            outputs,
            enabled: settings.skills.enabled.iter().any(|item| item == &id),
            chars: body.chars().count(),
        });
    }
    skills.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(skills)
}

pub fn set_enabled(
    app: &tauri::AppHandle,
    skill_id: &str,
    enabled: bool,
) -> Result<Vec<SkillInfo>, String> {
    let directory = skills_dir(app)?;
    let path = directory.join(format!("{skill_id}.md"));
    if !path.is_file() {
        return Err("技能不存在。".to_string());
    }
    let mut settings = settings::load_settings_for_app(app)?;
    settings.skills.enabled.retain(|item| item != skill_id);
    if enabled {
        settings.skills.enabled.push(skill_id.to_string());
    }
    settings::save_settings_for_app(app, &settings)?;
    list_skills(app, &settings)
}

pub fn enabled_prompts(
    app: &tauri::AppHandle,
    settings: &GlobalSettings,
) -> Result<Vec<(String, String)>, String> {
    let directory = skills_dir(app)?;
    let mut prompts = Vec::new();
    for id in &settings.skills.enabled {
        let path = directory.join(format!("{id}.md"));
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let (meta, body) = split_frontmatter(&text);
        let name = meta_value(&meta, "name").unwrap_or_else(|| id.clone());
        let body: String = body.chars().take(4_000).collect();
        prompts.push((name, body));
    }
    Ok(prompts)
}

fn split_frontmatter(text: &str) -> (String, String) {
    let normalized = text.replace("\r\n", "\n");
    if !normalized.starts_with("---\n") {
        return (String::new(), normalized);
    }
    let Some(end) = normalized[4..].find("\n---") else {
        return (String::new(), normalized);
    };
    let meta = normalized[4..4 + end].to_string();
    let body = normalized[4 + end + 4..]
        .trim_start_matches('\n')
        .to_string();
    (meta, body)
}

fn meta_value(meta: &str, key: &str) -> Option<String> {
    for line in meta.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case(key) {
            return Some(
                value
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string(),
            );
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_frontmatter_and_body() {
        let (meta, body) = split_frontmatter(SEED_SKILL);
        assert_eq!(meta_value(&meta, "name").as_deref(), Some("学术趋势研究"));
        assert!(body.contains("学术检索"));
    }
}

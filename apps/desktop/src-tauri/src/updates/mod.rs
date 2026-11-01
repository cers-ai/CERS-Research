use serde::Serialize;

use crate::settings::{self, GlobalSettings};

const RELEASE_API: &str = "https://api.github.com/repos/cers-ai/CERS-Research/releases/latest";
const MAX_NOTES_CHARS: usize = 20_000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub has_update: bool,
    pub skipped: bool,
    pub release_url: Option<String>,
    pub notes: Option<String>,
    pub published_at: Option<String>,
    pub checked_at: String,
    pub message: Option<String>,
}

pub fn check_for_updates(app: &tauri::AppHandle, force: bool) -> Result<UpdateInfo, String> {
    let mut settings = settings::load_settings_for_app(app)?;
    let current = env!("CARGO_PKG_VERSION").to_string();
    let checked_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

    if !force && !settings.updates.auto_check {
        return Ok(UpdateInfo {
            current_version: current.clone(),
            latest_version: current,
            has_update: false,
            skipped: false,
            release_url: None,
            notes: None,
            published_at: None,
            checked_at,
            message: Some("启动自动检查已关闭。".to_string()),
        });
    }

    settings.updates.last_checked_at = Some(checked_at.clone());
    let _ = settings::save_settings_for_app(app, &settings);

    let proxy = settings.network.proxy_url.clone();
    let timeout = settings.network.timeout_seconds;
    let agent =
        settings::http_agent(proxy.as_deref(), timeout).map_err(|error| error.to_string())?;

    let response = agent
        .get(RELEASE_API)
        .set("Accept", "application/vnd.github+json")
        .set("User-Agent", &format!("CERS-Academic/{current}"))
        .call();

    let value = match response {
        Ok(response) => {
            let text = response.into_string().unwrap_or_default();
            serde_json::from_str::<serde_json::Value>(&text)
                .map_err(|error| format!("更新信息格式无效：{error}"))?
        }
        Err(ureq::Error::Status(404, _)) => {
            return Ok(UpdateInfo {
                current_version: current.clone(),
                latest_version: current,
                has_update: false,
                skipped: false,
                release_url: None,
                notes: None,
                published_at: None,
                checked_at,
                message: Some("GitHub 上还没有正式发布版本。".to_string()),
            });
        }
        Err(ureq::Error::Status(code, response)) => {
            let text = response.into_string().unwrap_or_default();
            let detail = text.chars().take(120).collect::<String>();
            return Ok(UpdateInfo {
                current_version: current.clone(),
                latest_version: current,
                has_update: false,
                skipped: false,
                release_url: None,
                notes: None,
                published_at: None,
                checked_at,
                message: Some(format!("更新服务返回 HTTP {code}：{detail}")),
            });
        }
        Err(ureq::Error::Transport(error)) => {
            return Ok(UpdateInfo {
                current_version: current.clone(),
                latest_version: current,
                has_update: false,
                skipped: false,
                release_url: None,
                notes: None,
                published_at: None,
                checked_at,
                message: Some(format!("无法连接更新服务：{error}")),
            });
        }
    };

    let tag = value
        .get("tag_name")
        .and_then(|item| item.as_str())
        .unwrap_or("")
        .trim()
        .trim_start_matches('v')
        .to_string();
    if tag.is_empty() {
        return Ok(UpdateInfo {
            current_version: current.clone(),
            latest_version: current,
            has_update: false,
            skipped: false,
            release_url: None,
            notes: None,
            published_at: None,
            checked_at,
            message: Some("更新服务没有返回有效版本号。".to_string()),
        });
    }

    let comparable = version_is_newer(&tag, &current);
    let skipped = settings
        .updates
        .skipped_version
        .as_deref()
        .map(|value| value.trim_start_matches('v') == tag)
        .unwrap_or(false);

    Ok(UpdateInfo {
        current_version: current,
        latest_version: tag.clone(),
        has_update: comparable && !skipped,
        skipped,
        release_url: value
            .get("html_url")
            .and_then(|item| item.as_str())
            .map(str::to_string),
        notes: value
            .get("body")
            .and_then(|item| item.as_str())
            .map(|body| body.chars().take(MAX_NOTES_CHARS).collect()),
        published_at: value
            .get("published_at")
            .and_then(|item| item.as_str())
            .map(str::to_string),
        checked_at,
        message: if comparable {
            None
        } else {
            Some("当前已是最新版本。".to_string())
        },
    })
}

fn version_is_newer(latest: &str, current: &str) -> bool {
    let latest = semver::Version::parse(latest.trim_start_matches('v'));
    let current = semver::Version::parse(current.trim_start_matches('v'));
    match (latest, current) {
        (Ok(latest), Ok(current)) => latest > current,
        _ => false,
    }
}

pub fn skip_version(app: &tauri::AppHandle, version: &str) -> Result<(), String> {
    let mut settings = settings::load_settings_for_app(app)?;
    settings.updates.skipped_version = Some(version.trim().trim_start_matches('v').to_string());
    settings::save_settings_for_app(app, &settings)
}

pub fn set_auto_check(app: &tauri::AppHandle, auto_check: bool) -> Result<GlobalSettings, String> {
    let mut settings = settings::load_settings_for_app(app)?;
    settings.updates.auto_check = auto_check;
    settings::save_settings_for_app(app, &settings)?;
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::version_is_newer;

    #[test]
    fn compares_versions() {
        assert!(version_is_newer("0.0.2", "0.0.1"));
        assert!(version_is_newer("v1.2.0", "1.1.9"));
        assert!(!version_is_newer("0.0.1", "0.0.1"));
        assert!(!version_is_newer("0.0.0", "0.0.1"));
        assert!(!version_is_newer("nightly", "0.0.1"));
    }
}

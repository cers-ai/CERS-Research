use std::fs;
use std::path::{Component, Path, PathBuf};

use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::WorkspaceInfo;

pub mod asset_links;
pub mod assets;
pub mod backup;
pub mod chat;
pub mod definition;
pub mod experiments;
pub mod graph;
pub mod proposals;
pub mod reports;
pub mod sources;

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("无法访问所选文件夹：{0}")]
    Io(#[from] std::io::Error),
    #[error("所选路径不是文件夹：{0}")]
    NotDirectory(String),
    #[error("项目配置文件 .cers 是一个目录，无法作为配置读取：{0}")]
    CersIsDirectory(String),
    #[error("无法解析 .cers（{path}）：{source}")]
    CersParse {
        path: String,
        source: serde_json::Error,
    },
    #[error("项目配置或数据版本 {0} 暂不支持")]
    UnsupportedSchema(u32),
    #[error("项目数据与 .cers 不一致：配置为 {expected}，数据库为 {actual}")]
    ProjectMismatch { expected: String, actual: String },
    #[error("项目配置中的路径越出了 Workspace：{0}")]
    PathEscape(String),
    #[error("本地项目数据库操作失败：{0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("无法写入项目配置：{0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CersConfig {
    schema_version: u32,
    project_id: String,
    #[serde(default)]
    display_name: Option<String>,
    created_at: String,
    storage: CersStorage,
    #[serde(default)]
    settings_overrides: CersSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CersStorage {
    project_db: String,
    output_root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CersSettings {
    preferred_language: String,
}

impl Default for CersSettings {
    fn default() -> Self {
        Self {
            preferred_language: "zh-CN".to_string(),
        }
    }
}

pub fn open_workspace(selected: &Path) -> Result<WorkspaceInfo, StorageError> {
    let root = dunce::canonicalize(selected)?;
    if !root.is_dir() {
        return Err(StorageError::NotDirectory(root.display().to_string()));
    }

    let folder_name = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("未命名项目")
        .to_string();

    let cers_path = root.join(".cers");
    let config = load_or_create_config(&root, &cers_path)?;

    let data_dir = root.join(".cers-data");
    for sub in ["runs", "cache", "snapshots"] {
        fs::create_dir_all(data_dir.join(sub))?;
    }

    let output_root = safe_join(&root, &config.storage.output_root)?;
    for sub in ["research", "experiments", "reports"] {
        fs::create_dir_all(output_root.join(sub))?;
    }

    let db_path = safe_join(&root, &config.storage.project_db)?;
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut connection = Connection::open(&db_path)?;
    connection.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;",
    )?;
    migrate(&connection)?;

    let alias = config
        .display_name
        .clone()
        .filter(|value| !value.trim().is_empty());
    let project_name = alias.clone().unwrap_or_else(|| folder_name.clone());
    let root_node_id = ensure_project(
        &mut connection,
        &config.project_id,
        &project_name,
        alias.as_deref(),
        &root,
    )?;

    Ok(WorkspaceInfo {
        project_id: config.project_id,
        name: folder_name,
        alias,
        workspace_path: root.display().to_string(),
        root_node_id,
        created_at: config.created_at,
        schema_version: SCHEMA_VERSION,
    })
}

fn load_or_create_config(root: &Path, cers_path: &Path) -> Result<CersConfig, StorageError> {
    if cers_path.exists() {
        if cers_path.is_dir() {
            return Err(StorageError::CersIsDirectory(
                cers_path.display().to_string(),
            ));
        }
        let text = fs::read_to_string(cers_path)?;
        let config: CersConfig =
            serde_json::from_str(&text).map_err(|source| StorageError::CersParse {
                path: cers_path.display().to_string(),
                source,
            })?;
        if config.schema_version != SCHEMA_VERSION {
            return Err(StorageError::UnsupportedSchema(config.schema_version));
        }
        return Ok(config);
    }

    let config = CersConfig {
        schema_version: SCHEMA_VERSION,
        project_id: new_id("prj"),
        display_name: None,
        created_at: now_iso(),
        storage: CersStorage {
            project_db: ".cers-data/project.sqlite".to_string(),
            output_root: "Research Outputs".to_string(),
        },
        settings_overrides: CersSettings::default(),
    };

    let temp_path = root.join(".cers.tmp");
    fs::write(&temp_path, serde_json::to_string_pretty(&config)?)?;
    fs::rename(&temp_path, cers_path)?;
    Ok(config)
}

fn ensure_column(
    connection: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), StorageError> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let exists = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|name| name == column);
    drop(statement);
    if !exists {
        connection.execute_batch(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {definition}"
        ))?;
    }
    Ok(())
}

fn migrate(connection: &Connection) -> Result<(), StorageError> {
    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS app_meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS project (
            id              TEXT PRIMARY KEY,
            name            TEXT NOT NULL,
            alias           TEXT,
            workspace_path  TEXT NOT NULL,
            schema_version  INTEGER NOT NULL,
            created_at      TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS node (
            id          TEXT PRIMARY KEY,
            project_id  TEXT NOT NULL,
            type        TEXT NOT NULL CHECK (type IN ('root','research','experiment','report')),
            title       TEXT NOT NULL,
            goal        TEXT NOT NULL DEFAULT '',
            state       TEXT NOT NULL DEFAULT 'ready'
                        CHECK (state IN ('ready','running','waiting_approval','attention','result_available')),
            pos_x       REAL NOT NULL DEFAULT 0,
            pos_y       REAL NOT NULL DEFAULT 0,
            revision    INTEGER NOT NULL DEFAULT 1,
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL,
            FOREIGN KEY (project_id) REFERENCES project(id) ON DELETE CASCADE
        );

        CREATE UNIQUE INDEX IF NOT EXISTS ux_node_single_root
            ON node(project_id) WHERE type = 'root';

        CREATE TABLE IF NOT EXISTS edge (
            id          TEXT PRIMARY KEY,
            project_id  TEXT NOT NULL,
            source_id   TEXT NOT NULL,
            target_id   TEXT NOT NULL,
            kind        TEXT NOT NULL CHECK (kind IN ('decomposition','association')),
            label       TEXT,
            created_at  TEXT NOT NULL,
            FOREIGN KEY (project_id) REFERENCES project(id) ON DELETE CASCADE,
            FOREIGN KEY (source_id) REFERENCES node(id) ON DELETE CASCADE,
            FOREIGN KEY (target_id) REFERENCES node(id) ON DELETE CASCADE,
            UNIQUE (source_id, target_id, kind)
        );

        CREATE TABLE IF NOT EXISTS proposal (
            id                  TEXT PRIMARY KEY,
            project_id          TEXT NOT NULL,
            anchor_node_id      TEXT NOT NULL,
            base_graph_revision INTEGER NOT NULL,
            status              TEXT NOT NULL
                                CHECK (status IN ('draft','awaiting_approval','approved','rejected','applied')),
            payload_json        TEXT NOT NULL,
            created_at          TEXT NOT NULL,
            updated_at          TEXT NOT NULL,
            FOREIGN KEY (project_id) REFERENCES project(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS asset (
            id           TEXT PRIMARY KEY,
            project_id   TEXT NOT NULL,
            path         TEXT NOT NULL,
            kind         TEXT NOT NULL,
            status       TEXT NOT NULL
                         CHECK (status IN ('indexed','parsing','unsupported','missing','relocated','error')),
            content_hash TEXT,
            origin       TEXT NOT NULL,
            created_at   TEXT NOT NULL,
            updated_at   TEXT NOT NULL,
            FOREIGN KEY (project_id) REFERENCES project(id) ON DELETE CASCADE
        );

        CREATE UNIQUE INDEX IF NOT EXISTS ux_asset_project_path
            ON asset(project_id, path);

        CREATE VIRTUAL TABLE IF NOT EXISTS asset_fts USING fts5(
            asset_id UNINDEXED,
            name,
            content,
            tokenize = 'trigram'
        );

        CREATE TABLE IF NOT EXISTS job (
            id          TEXT PRIMARY KEY,
            project_id  TEXT NOT NULL,
            node_id     TEXT,
            kind        TEXT NOT NULL,
            status      TEXT NOT NULL
                        CHECK (status IN ('queued','running','waiting_approval','paused','retry_pending','completed','failed','interrupted','cancelled')),
            input_json  TEXT NOT NULL DEFAULT '{}',
            created_at  TEXT NOT NULL,
            started_at  TEXT,
            finished_at TEXT,
            FOREIGN KEY (project_id) REFERENCES project(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS conversation (
            id          TEXT PRIMARY KEY,
            project_id  TEXT NOT NULL,
            node_id     TEXT NOT NULL,
            role        TEXT NOT NULL CHECK (role IN ('user','assistant','system')),
            body        TEXT NOT NULL,
            meta_json   TEXT,
            created_at  TEXT NOT NULL,
            FOREIGN KEY (project_id) REFERENCES project(id) ON DELETE CASCADE,
            FOREIGN KEY (node_id) REFERENCES node(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS ix_conversation_node
            ON conversation(node_id, created_at);

        CREATE TABLE IF NOT EXISTS research_definition (
            project_id      TEXT PRIMARY KEY,
            content_json    TEXT NOT NULL,
            confirmed_fields TEXT NOT NULL DEFAULT '[]',
            revision        INTEGER NOT NULL DEFAULT 1,
            updated_at      TEXT NOT NULL,
            FOREIGN KEY (project_id) REFERENCES project(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS source_record (
            id             TEXT PRIMARY KEY,
            project_id     TEXT NOT NULL,
            provider       TEXT NOT NULL,
            provider_name  TEXT NOT NULL,
            external_id    TEXT,
            dedup_key      TEXT NOT NULL,
            doi            TEXT,
            url            TEXT,
            title          TEXT NOT NULL,
            authors_json   TEXT NOT NULL DEFAULT '[]',
            year           INTEGER,
            venue          TEXT,
            abstract       TEXT,
            coverage       TEXT NOT NULL
                           CHECK (coverage IN ('metadata_only','abstract_only','fulltext','unavailable')),
            record_type    TEXT,
            retrieved_at   TEXT,
            created_at     TEXT NOT NULL,
            FOREIGN KEY (project_id) REFERENCES project(id) ON DELETE CASCADE,
            UNIQUE (project_id, dedup_key)
        );

        CREATE TABLE IF NOT EXISTS source_node_link (
            source_id   TEXT NOT NULL,
            node_id     TEXT NOT NULL,
            created_at  TEXT NOT NULL,
            PRIMARY KEY (source_id, node_id),
            FOREIGN KEY (source_id) REFERENCES source_record(id) ON DELETE CASCADE,
            FOREIGN KEY (node_id) REFERENCES node(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS report_document (
            id          TEXT PRIMARY KEY,
            project_id  TEXT NOT NULL,
            node_id     TEXT NOT NULL UNIQUE,
            content_md  TEXT NOT NULL DEFAULT '',
            revision    INTEGER NOT NULL DEFAULT 0,
            updated_at  TEXT NOT NULL,
            FOREIGN KEY (project_id) REFERENCES project(id) ON DELETE CASCADE,
            FOREIGN KEY (node_id) REFERENCES node(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS report_revision (
            id          TEXT PRIMARY KEY,
            report_id   TEXT NOT NULL,
            revision    INTEGER NOT NULL,
            content_md  TEXT NOT NULL,
            created_at  TEXT NOT NULL,
            UNIQUE (report_id, revision),
            FOREIGN KEY (report_id) REFERENCES report_document(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS report_change_proposal (
            id             TEXT PRIMARY KEY,
            project_id     TEXT NOT NULL,
            report_id      TEXT NOT NULL,
            node_id        TEXT NOT NULL,
            base_revision  INTEGER NOT NULL,
            status         TEXT NOT NULL
                           CHECK (status IN ('pending','partially_applied','applied','rejected','conflict')),
            payload_json   TEXT NOT NULL,
            created_at     TEXT NOT NULL,
            updated_at     TEXT NOT NULL,
            FOREIGN KEY (project_id) REFERENCES project(id) ON DELETE CASCADE,
            FOREIGN KEY (report_id) REFERENCES report_document(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS asset_node_link (
            project_id  TEXT NOT NULL,
            asset_id    TEXT NOT NULL,
            node_id     TEXT NOT NULL,
            created_at  TEXT NOT NULL,
            PRIMARY KEY (asset_id, node_id),
            FOREIGN KEY (project_id) REFERENCES project(id) ON DELETE CASCADE,
            FOREIGN KEY (asset_id) REFERENCES asset(id) ON DELETE CASCADE,
            FOREIGN KEY (node_id) REFERENCES node(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS experiment_run (
            id              TEXT PRIMARY KEY,
            project_id      TEXT NOT NULL,
            node_id         TEXT NOT NULL,
            asset_id        TEXT,
            input_path      TEXT,
            goal            TEXT NOT NULL,
            parameters_json TEXT NOT NULL DEFAULT '{}',
            state           TEXT NOT NULL CHECK (state IN ('completed','failed')),
            outputs_json    TEXT NOT NULL,
            error           TEXT,
            created_at      TEXT NOT NULL,
            finished_at     TEXT,
            FOREIGN KEY (project_id) REFERENCES project(id) ON DELETE CASCADE,
            FOREIGN KEY (node_id) REFERENCES node(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS ix_experiment_run_node
            ON experiment_run(node_id, created_at);

        CREATE TABLE IF NOT EXISTS job_event (
            job_id       TEXT NOT NULL,
            seq          INTEGER NOT NULL,
            type         TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            created_at   TEXT NOT NULL,
            PRIMARY KEY (job_id, seq),
            FOREIGN KEY (job_id) REFERENCES job(id) ON DELETE CASCADE
        );
        "#,
    )?;

    ensure_column(connection, "conversation", "meta_json", "TEXT")?;

    let current: Option<String> = connection
        .query_row(
            "SELECT value FROM app_meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .optional()?;

    match current {
        None => {
            connection.execute(
                "INSERT INTO app_meta (key, value) VALUES ('schema_version', ?1)",
                params![SCHEMA_VERSION.to_string()],
            )?;
        }
        Some(value) if value == SCHEMA_VERSION.to_string() => {}
        Some(value) => {
            return Err(StorageError::UnsupportedSchema(value.parse().unwrap_or(0)));
        }
    }

    connection.execute(
        "INSERT OR IGNORE INTO app_meta (key, value) VALUES ('graph_revision', '0')",
        [],
    )?;

    Ok(())
}

pub fn project_db_path(workspace: &WorkspaceInfo) -> PathBuf {
    Path::new(&workspace.workspace_path)
        .join(".cers-data")
        .join("project.sqlite")
}

pub fn open_project_db(workspace: &WorkspaceInfo) -> Result<Connection, StorageError> {
    let connection = Connection::open(project_db_path(workspace))?;
    connection.execute_batch("PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;")?;
    Ok(connection)
}

fn ensure_project(
    connection: &mut Connection,
    project_id: &str,
    project_name: &str,
    alias: Option<&str>,
    root: &Path,
) -> Result<String, StorageError> {
    let transaction = connection.transaction()?;

    let existing: Option<String> = transaction
        .query_row("SELECT id FROM project LIMIT 1", [], |row| row.get(0))
        .optional()?;

    if let Some(actual) = existing {
        if actual != project_id {
            return Err(StorageError::ProjectMismatch {
                expected: project_id.to_string(),
                actual,
            });
        }
    }

    transaction.execute(
        "INSERT INTO project (id, name, alias, workspace_path, schema_version, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(id) DO UPDATE SET
             name = excluded.name,
             alias = excluded.alias,
             workspace_path = excluded.workspace_path",
        params![
            project_id,
            project_name,
            alias,
            root.display().to_string(),
            SCHEMA_VERSION,
            now_iso()
        ],
    )?;

    let root_node: Option<String> = transaction
        .query_row(
            "SELECT id FROM node WHERE project_id = ?1 AND type = 'root' LIMIT 1",
            params![project_id],
            |row| row.get(0),
        )
        .optional()?;

    if let Some(id) = root_node {
        transaction.commit()?;
        return Ok(id);
    }

    let root_id = new_id("node");
    let now = now_iso();
    transaction.execute(
        "INSERT INTO node (id, project_id, type, title, goal, state, pos_x, pos_y, revision, created_at, updated_at)
         VALUES (?1, ?2, 'root', ?3, '', 'ready', 0, 0, 1, ?4, ?4)",
        params![root_id, project_id, project_name, now],
    )?;
    transaction.commit()?;
    Ok(root_id)
}

fn safe_join(root: &Path, relative: &str) -> Result<PathBuf, StorageError> {
    let mut candidate = root.to_path_buf();
    for component in Path::new(relative).components() {
        match component {
            Component::Normal(part) => candidate.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                if !candidate.pop() {
                    return Err(StorageError::PathEscape(relative.to_string()));
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(StorageError::PathEscape(relative.to_string()));
            }
        }
    }
    if candidate.starts_with(root) {
        Ok(candidate)
    } else {
        Err(StorageError::PathEscape(relative.to_string()))
    }
}

pub fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn new_id(prefix: &str) -> String {
    format!(
        "{}_{}",
        prefix,
        ulid::Ulid::new().to_string().to_lowercase()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_project_and_keeps_single_root() {
        let directory = tempfile::tempdir().expect("创建临时目录");
        let first = open_workspace(directory.path()).expect("首次打开项目");

        assert!(directory.path().join(".cers").is_file());
        assert!(directory.path().join(".cers-data/project.sqlite").is_file());
        assert!(directory.path().join("Research Outputs/research").is_dir());

        let second = open_workspace(directory.path()).expect("再次打开项目");
        assert_eq!(first.project_id, second.project_id);
        assert_eq!(first.root_node_id, second.root_node_id);

        let connection = Connection::open(directory.path().join(".cers-data/project.sqlite"))
            .expect("打开数据库");
        let roots: i64 = connection
            .query_row("SELECT COUNT(*) FROM node WHERE type = 'root'", [], |row| {
                row.get(0)
            })
            .expect("统计根节点");
        assert_eq!(roots, 1);
    }

    #[test]
    fn rejects_unsupported_schema_version() {
        let directory = tempfile::tempdir().expect("创建临时目录");
        fs::write(
            directory.path().join(".cers"),
            r#"{
              "schema_version": 99,
              "project_id": "prj_test",
              "created_at": "2026-10-28T00:00:00Z",
              "storage": {
                "project_db": ".cers-data/project.sqlite",
                "output_root": "Research Outputs"
              }
            }"#,
        )
        .expect("写入测试配置");

        let error = open_workspace(directory.path()).expect_err("应拒绝未知版本");
        assert!(matches!(error, StorageError::UnsupportedSchema(99)));
    }
}

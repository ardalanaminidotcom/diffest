pub mod category_settings;
mod git;

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub struct Application {
    database: PathBuf,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Registration {
    pub checkout: PathBuf,
    pub project: Project,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub main_checkout: PathBuf,
    pub worktrees: Vec<Worktree>,
    pub worktree_error: Option<String>,
}

#[derive(Serialize)]
pub struct Worktree {
    pub path: PathBuf,
    pub branch: Option<String>,
}

#[derive(Serialize)]
pub struct Revisions {
    pub branches: Vec<String>,
    pub commits: Vec<Commit>,
}

#[derive(Serialize)]
pub struct Commit {
    pub id: String,
    pub subject: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonMode {
    Between,
    Divergence,
}

#[derive(Serialize)]
pub struct Comparison {
    pub start: String,
    pub end: String,
    pub files: Vec<ChangedFile>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFile {
    pub status: char,
    pub path: String,
    pub previous_path: Option<String>,
    pub patch: String,
}

impl Project {
    fn new(id: i64, main_checkout: PathBuf) -> Self {
        let name = main_checkout
            .file_name()
            .unwrap_or(main_checkout.as_os_str())
            .to_string_lossy()
            .into_owned();
        Self {
            id,
            name,
            main_checkout,
            worktrees: Vec::new(),
            worktree_error: None,
        }
    }
}

impl Application {
    pub fn new(database: PathBuf) -> Self {
        Self { database }
    }

    pub fn register_project(&self, checkout: &Path) -> Result<Registration, String> {
        let checkout = git::checkout_root(checkout)?;
        let common_directory = git::common_directory(&checkout)?;
        let (mut main_checkout, mut worktrees) = git::worktrees(&common_directory, &checkout)?;
        let (id, registered_main): (i64, String) = self
            .connect()?
            .query_row(
                "INSERT INTO projects (common_directory, main_checkout) VALUES (?1, ?2)
                 ON CONFLICT (common_directory) DO UPDATE SET main_checkout = COALESCE(
                    NULLIF(excluded.main_checkout, excluded.common_directory), projects.main_checkout
                 )
                 RETURNING id, main_checkout",
                params![path_text(&common_directory)?, path_text(&main_checkout)?],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|error| format!("Cannot register project: {error}"))?;
        if Path::new(&registered_main) != main_checkout {
            (main_checkout, worktrees) =
                git::worktrees(&common_directory, Path::new(&registered_main))?;
        }
        let mut project = Project::new(id, main_checkout);
        project.worktrees = worktrees;
        Ok(Registration { project, checkout })
    }

    pub fn list_projects(&self) -> Result<Vec<Project>, String> {
        let connection = self.connect()?;
        let read_projects = || {
            let mut query = connection
                .prepare("SELECT id, common_directory, main_checkout FROM projects ORDER BY id")?;
            query
                .query_map([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()
        };
        let rows = read_projects().map_err(|error| format!("Cannot list projects: {error}"))?;
        Ok(rows
            .into_iter()
            .map(|(id, common_directory, main_checkout)| {
                match git::worktrees(Path::new(&common_directory), Path::new(&main_checkout)) {
                    Ok((main_checkout, worktrees)) => {
                        let mut project = Project::new(id, main_checkout);
                        project.worktrees = worktrees;
                        project
                    }
                    Err(error) => {
                        let mut project = Project::new(id, main_checkout.into());
                        project.worktree_error = Some(error);
                        project
                    }
                }
            })
            .collect())
    }

    pub fn open_worktree(&self, checkout: &Path) -> Result<Revisions, String> {
        git::revisions(&git::checkout_root(checkout)?)
    }

    pub fn compare_revisions(
        &self,
        checkout: &Path,
        base: &str,
        target: &str,
        mode: ComparisonMode,
    ) -> Result<Comparison, String> {
        let checkout = git::checkout_root(checkout)?;
        let base = git::resolve_commit(&checkout, base)?;
        let end = git::resolve_commit(&checkout, target)?;
        let start = match mode {
            ComparisonMode::Between => base,
            ComparisonMode::Divergence => git::merge_base(&checkout, &base, &end)?,
        };
        let files = git::compare_commits(&checkout, &start, &end)?;
        Ok(Comparison { start, end, files })
    }

    fn connect(&self) -> Result<Connection, String> {
        if let Some(parent) = self.database.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Cannot create app data directory: {error}"))?;
        }
        let open_database = || {
            let connection = Connection::open(&self.database)?;
            connection.execute_batch(
                "CREATE TABLE IF NOT EXISTS projects (
                    id INTEGER PRIMARY KEY,
                    common_directory TEXT NOT NULL UNIQUE,
                    main_checkout TEXT NOT NULL
                )",
            )?;
            Ok::<_, rusqlite::Error>(connection)
        };
        open_database().map_err(|error| format!("Cannot open {}: {error}", self.database.display()))
    }
}

fn path_text(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| format!("Path is not UTF-8: {}", path.display()))
}

pub mod category_settings;
mod comparison_reviews;
mod diff_layout;
mod file_details;
mod git;
mod reviews;
mod worktree_review;

pub use comparison_reviews::{ComparisonUpdate, ReviewRetention, ReviewSelection};
pub use diff_layout::DiffLayout;
pub use file_details::{FileDetails, FileDetailsSource};
pub use reviews::{CommitReview, CommitReviewProgress, CommitReviewSummary};
pub use worktree_review::{PatchSide, PatchVersion, StageWorktreePatchResult, WorktreePatch};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

pub struct Application {
    database: PathBuf,
    active_review: Mutex<Option<i64>>,
    active_snapshot: Mutex<Option<Arc<ActiveSnapshot>>>,
}

struct ActiveSnapshot {
    _storage: tempfile::TempDir,
    objects: git::ReviewObjects,
    checkout: PathBuf,
    end: String,
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
    pub main_checkout: Option<PathBuf>,
    pub worktrees: Vec<Worktree>,
    pub worktree_error: Option<String>,
}

#[derive(Serialize)]
pub struct Worktree {
    pub path: PathBuf,
    pub branch: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Revisions {
    pub branches: Vec<String>,
    pub remote_branches: Vec<String>,
    pub synced_remote_branches: Vec<String>,
    pub tags: Vec<String>,
    pub default_branch: Option<String>,
    pub current_branch: Option<String>,
    pub commits: Vec<Commit>,
}

#[derive(Serialize)]
pub struct Commit {
    pub id: String,
    pub subject: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonMode {
    Between,
    Divergence,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Comparison {
    pub base_label: String,
    pub target_label: String,
    pub mode: ComparisonMode,
    pub base: String,
    pub start: String,
    pub end: String,
    pub files: Vec<ChangedFile>,
}

#[derive(Serialize)]
pub struct ComparisonSession {
    #[serde(flatten)]
    pub comparison: Comparison,
    pub progress: Option<CommitReviewProgress>,
    pub selection: ReviewSelection,
    pub update: ComparisonUpdate,
    pub layout: Option<DiffLayout>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFile {
    pub status: char,
    pub path: String,
    pub previous_path: Option<String>,
    // IPC uses readable labels, with the original bytes attached only when needed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_bytes: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_path_bytes: Option<Vec<u8>>,
    pub patch: String,
}

impl ChangedFile {
    fn new(status: char, path: &[u8], previous_path: Option<&[u8]>, patch: String) -> Self {
        Self {
            status,
            path: git::display_path(path),
            previous_path: previous_path.map(git::display_path),
            path_bytes: invalid_utf8(path),
            previous_path_bytes: previous_path.and_then(invalid_utf8),
            patch,
        }
    }

    fn path_bytes(&self) -> &[u8] {
        self.path_bytes.as_deref().unwrap_or(self.path.as_bytes())
    }

    fn previous_path_bytes(&self) -> Option<&[u8]> {
        self.previous_path_bytes
            .as_deref()
            .or_else(|| self.previous_path.as_deref().map(str::as_bytes))
    }
}

fn invalid_utf8(path: &[u8]) -> Option<Vec<u8>> {
    std::str::from_utf8(path).is_err().then(|| path.to_vec())
}

impl Project {
    fn new(id: i64, common_directory: &Path, main_checkout: Option<PathBuf>) -> Self {
        let project_path = main_checkout.as_deref().unwrap_or(common_directory);
        let name = project_path
            .file_name()
            .unwrap_or(project_path.as_os_str())
            .to_string_lossy()
            .into_owned();
        let worktree_error = main_checkout.is_none().then(|| {
            "Git did not identify a main checkout. Add the main checkout to enable project settings."
                .into()
        });
        Self {
            id,
            name,
            main_checkout,
            worktrees: Vec::new(),
            worktree_error,
        }
    }
}

impl Application {
    pub fn new(database: PathBuf) -> Self {
        Self {
            database,
            active_review: Default::default(),
            active_snapshot: Default::default(),
        }
    }

    pub fn register_project(&self, checkout: &Path) -> Result<Registration, String> {
        let checkout = git::checkout_root(checkout)?;
        let common_directory = git::common_directory(&checkout)?;
        let (mut main_checkout, mut worktrees) = git::worktrees(&common_directory, &checkout)?;
        let connection = self.connect()?;
        let (id, registered_main): (i64, String) = connection.query_row(
                "INSERT INTO projects (common_directory, main_checkout) VALUES (?1, ?2)
                 ON CONFLICT (common_directory) DO UPDATE SET main_checkout = COALESCE(
                    NULLIF(excluded.main_checkout, excluded.common_directory), projects.main_checkout
                 )
                 RETURNING id, main_checkout",
                params![
                    path_text(&common_directory)?,
                    path_text(main_checkout.as_deref().unwrap_or(&common_directory))?,
                ],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|error| format!("Cannot register project: {error}"))?;
        if main_checkout.is_none() && Path::new(&registered_main) != common_directory {
            (main_checkout, worktrees) =
                git::worktrees(&common_directory, Path::new(&registered_main))?;
        }
        let mut project = Project::new(id, &common_directory, main_checkout);
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
        rows.into_iter()
            .map(|(id, common_directory, main_checkout)| {
                let common_directory = Path::new(&common_directory);
                let project = match git::worktrees(common_directory, Path::new(&main_checkout)) {
                    Ok((main_checkout, worktrees)) => {
                        let mut project = Project::new(id, common_directory, main_checkout);
                        project.worktrees = worktrees;
                        project
                    }
                    Err(error) => {
                        let mut project = Project::new(id, common_directory, None);
                        project.worktree_error = Some(error);
                        project
                    }
                };
                Ok(project)
            })
            .collect()
    }

    pub fn require_registered_checkout(
        &self,
        checkout: &Path,
        require_main: bool,
    ) -> Result<PathBuf, String> {
        let denied = "Select a registered Diffest checkout.";
        let supplied = checkout.canonicalize().map_err(|_| denied)?;
        let root = git::checkout_root(&supplied).map_err(|_| denied)?;
        if supplied != root {
            return Err(denied.into());
        }
        let common = git::common_directory(&root).map_err(|_| denied)?;
        let main: String = self
            .connect()?
            .query_row(
                "SELECT main_checkout FROM projects WHERE common_directory = ?1",
                [path_text(&common)?],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| format!("Cannot check project registration: {error}"))?
            .ok_or(denied)?;
        let (main, worktrees) = git::worktrees(&common, Path::new(&main))?;
        let registered = if require_main {
            main.as_deref() == Some(root.as_path())
        } else {
            worktrees.iter().any(|worktree| worktree.path == root)
        };
        if !registered {
            return Err(denied.into());
        }
        if require_main {
            for path in [root.join(".diffest"), root.join(".diffest/settings.json")] {
                match path.symlink_metadata() {
                    Ok(metadata) if metadata.is_symlink() => {
                        return Err("Project settings cannot use symbolic links.".into());
                    }
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(format!("Cannot inspect project settings: {error}")),
                }
            }
        }
        Ok(root)
    }

    pub fn open_worktree(&self, checkout: &Path) -> Result<Revisions, String> {
        git::revisions(&git::checkout_root(checkout)?)
    }

    pub fn fetch_revisions(&self, checkout: &Path) -> Result<Revisions, String> {
        let checkout = self.require_registered_checkout(checkout, false)?;
        git::fetch_branches(&checkout)?;
        self.check_deleted_reviews(&checkout)?;
        git::revisions(&checkout)
    }

    pub fn compare_revisions(
        &self,
        checkout: &Path,
        base: &str,
        target: &str,
        mode: ComparisonMode,
    ) -> Result<Comparison, String> {
        let checkout = git::checkout_root(checkout)?;
        let objects = (target == "Working tree")
            .then(|| self.review_objects(&checkout))
            .transpose()?;
        Self::compare_with_objects(&checkout, base, target, mode, objects.as_ref())
    }

    fn compare_with_objects(
        checkout: &Path,
        base: &str,
        target: &str,
        mode: ComparisonMode,
        objects: Option<&git::ReviewObjects>,
    ) -> Result<Comparison, String> {
        let base_label = base.into();
        let target_label = target.into();
        let base = git::resolve_commit(checkout, base)?;
        let head = if objects.is_some() { "HEAD" } else { target };
        let end = git::resolve_commit(checkout, head)?;
        let start = match mode {
            ComparisonMode::Between => base.clone(),
            ComparisonMode::Divergence => git::merge_base(checkout, &base, &end)?,
        };
        let end = match objects {
            Some(objects) => git::snapshot_worktree(checkout, objects)?,
            None => end,
        };
        let files = git::compare_commits(checkout, &start, &end, objects)?;
        Ok(Comparison {
            base_label,
            target_label,
            mode,
            base,
            start,
            end,
            files,
        })
    }

    fn review_objects(&self, checkout: &Path) -> Result<git::ReviewObjects, String> {
        let common_directory = git::common_directory(checkout)?;
        let key = Sha256::digest(path_text(&common_directory)?.as_bytes());
        let database = std::path::absolute(&self.database)
            .map_err(|error| format!("Cannot locate comparison storage: {error}"))?;
        Self::objects_with_alternate(
            &common_directory,
            database
                .with_extension("review-objects")
                .join(format!("{key:x}")),
        )
    }

    fn objects_with_alternate(
        common_directory: &Path,
        directory: PathBuf,
    ) -> Result<git::ReviewObjects, String> {
        let project_objects = common_directory.join("objects");
        let alternate = path_text(&project_objects)?
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r");
        Ok(git::ReviewObjects {
            directory,
            alternate: format!("\"{alternate}\""),
        })
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
            connection.execute_batch(reviews::SCHEMA)?;
            connection.execute_batch(diff_layout::SCHEMA)?;
            connection.execute_batch(comparison_reviews::SCHEMA)?;
            connection.execute_batch(worktree_review::SCHEMA)?;
            Ok::<_, rusqlite::Error>(connection)
        };
        open_database().map_err(|error| format!("Cannot open {}: {error}", self.database.display()))
    }
}

fn path_text(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| format!("Path is not UTF-8: {}", path.display()))
}

use crate::{Application, ChangedFile, Comparison, ComparisonMode, git, path_text};
use rusqlite::{Connection, params};
use serde::Serialize;
use std::path::Path;

pub(crate) const SCHEMA: &str = "
    PRAGMA foreign_keys = ON;
    CREATE TABLE IF NOT EXISTS commit_reviews (
        id INTEGER PRIMARY KEY,
        project_id INTEGER NOT NULL REFERENCES projects(id),
        base_label TEXT NOT NULL,
        target_label TEXT NOT NULL,
        divergence INTEGER NOT NULL,
        base_commit TEXT NOT NULL,
        start_commit TEXT NOT NULL,
        end_commit TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS commit_review_files (
        review_id INTEGER NOT NULL REFERENCES commit_reviews(id),
        file_index INTEGER NOT NULL,
        status INTEGER NOT NULL,
        path TEXT NOT NULL,
        previous_path TEXT,
        patch TEXT NOT NULL,
        reviewed INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (review_id, file_index)
    );";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitReviewSummary {
    pub id: i64,
    pub base_label: String,
    pub target_label: String,
    pub start: String,
    pub end: String,
    pub file_count: usize,
    pub reviewed_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitReview {
    pub comparison: Comparison,
    pub reviewed_files: Vec<usize>,
}

pub(crate) fn list(
    connection: &Connection,
    project_id: i64,
) -> Result<Vec<CommitReviewSummary>, String> {
    let list = || {
        let mut query = connection.prepare(
            "SELECT r.id, r.base_label, r.target_label, r.start_commit, r.end_commit,
                    COUNT(f.file_index), COUNT(*) FILTER (WHERE f.reviewed)
             FROM commit_reviews r LEFT JOIN commit_review_files f ON f.review_id = r.id
             WHERE r.project_id = ?1 GROUP BY r.id ORDER BY r.id DESC",
        )?;
        query
            .query_map([project_id], |row| {
                Ok(CommitReviewSummary {
                    id: row.get(0)?,
                    base_label: row.get(1)?,
                    target_label: row.get(2)?,
                    start: row.get(3)?,
                    end: row.get(4)?,
                    file_count: row.get(5)?,
                    reviewed_count: row.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()
    };
    list().map_err(|error| format!("Cannot list saved reviews: {error}"))
}

impl Application {
    pub fn save_commit_review(
        &self,
        checkout: &Path,
        comparison: &Comparison,
    ) -> Result<i64, String> {
        let mut connection = self.connect()?;
        let project = project_id(&connection, checkout)?;
        let mut save = || {
            let transaction = connection.transaction()?;
            let id = transaction.query_row(
                "INSERT INTO commit_reviews
                   (project_id, base_label, target_label, divergence, base_commit, start_commit, end_commit)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) RETURNING id",
                params![
                    project,
                    comparison.base_label,
                    comparison.target_label,
                    comparison.mode == ComparisonMode::Divergence,
                    comparison.base,
                    comparison.start,
                    comparison.end,
                ],
                |row| row.get(0),
            )?;
            // Keep the displayed patch even if another worktree has different Git attributes.
            let mut insert = transaction.prepare(
                "INSERT INTO commit_review_files
                   (review_id, file_index, status, path, previous_path, patch)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for (index, file) in comparison.files.iter().enumerate() {
                insert.execute(params![
                    id,
                    index,
                    file.status as u32,
                    file.path,
                    file.previous_path,
                    file.patch,
                ])?;
            }
            drop(insert);
            transaction.commit()?;
            Ok::<_, rusqlite::Error>(id)
        };
        save().map_err(|error| format!("Cannot save commit review: {error}"))
    }

    pub fn open_commit_review(&self, checkout: &Path, id: i64) -> Result<CommitReview, String> {
        let connection = self.connect()?;
        let project = project_id(&connection, checkout)?;
        let open = || {
            let mut comparison = connection.query_row(
                "SELECT base_label, target_label, divergence, base_commit, start_commit, end_commit
                 FROM commit_reviews WHERE id = ?1 AND project_id = ?2",
                params![id, project],
                |row| {
                    Ok(Comparison {
                        base_label: row.get(0)?,
                        target_label: row.get(1)?,
                        mode: if row.get(2)? {
                            ComparisonMode::Divergence
                        } else {
                            ComparisonMode::Between
                        },
                        base: row.get(3)?,
                        start: row.get(4)?,
                        end: row.get(5)?,
                        files: Vec::new(),
                    })
                },
            )?;
            let mut query = connection.prepare(
                "SELECT file_index, status, path, previous_path, patch, reviewed
                 FROM commit_review_files WHERE review_id = ?1 ORDER BY file_index",
            )?;
            let files = query.query_map([id], |row| {
                Ok((
                    row.get::<_, usize>(0)?,
                    row.get::<_, bool>(5)?,
                    ChangedFile {
                        status: char::from_u32(row.get(1)?).unwrap_or(char::REPLACEMENT_CHARACTER),
                        path: row.get(2)?,
                        previous_path: row.get(3)?,
                        patch: row.get(4)?,
                    },
                ))
            })?;
            let mut reviewed_files = Vec::new();
            for file in files {
                let (index, reviewed, file) = file?;
                if reviewed {
                    reviewed_files.push(index);
                }
                comparison.files.push(file);
            }
            Ok::<_, rusqlite::Error>(CommitReview {
                comparison,
                reviewed_files,
            })
        };
        open().map_err(|error| format!("Cannot open commit review: {error}"))
    }

    pub fn set_commit_file_reviewed(
        &self,
        checkout: &Path,
        id: i64,
        file_index: usize,
        reviewed: bool,
    ) -> Result<Vec<usize>, String> {
        let mut connection = self.connect()?;
        let project = project_id(&connection, checkout)?;
        let mut update = || {
            let transaction = connection.transaction()?;
            let changed = transaction.execute(
                "UPDATE commit_review_files SET reviewed = ?1
                 WHERE review_id = ?2 AND file_index = ?3 AND EXISTS (
                    SELECT 1 FROM commit_reviews WHERE id = ?2 AND project_id = ?4
                 )",
                params![reviewed, id, file_index, project],
            )?;
            if changed == 0 {
                return Err(rusqlite::Error::QueryReturnedNoRows);
            }
            let mut query = transaction.prepare(
                "SELECT file_index FROM commit_review_files
                 WHERE review_id = ?1 AND reviewed ORDER BY file_index",
            )?;
            let reviewed_files = query
                .query_map([id], |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            drop(query);
            transaction.commit()?;
            Ok(reviewed_files)
        };
        update().map_err(|error| format!("Cannot update review mark: {error}"))
    }
}

fn project_id(connection: &Connection, checkout: &Path) -> Result<i64, String> {
    let checkout = git::checkout_root(checkout)?;
    let common_directory = git::common_directory(&checkout)?;
    connection
        .query_row(
            "SELECT id FROM projects WHERE common_directory = ?1",
            [path_text(&common_directory)?],
            |row| row.get(0),
        )
        .map_err(|error| format!("Cannot find the registered project: {error}"))
}

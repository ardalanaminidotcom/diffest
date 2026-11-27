use crate::{Application, ChangedFile, Comparison, ComparisonMode, DiffLayout, git, path_text};
use rusqlite::{Connection, TransactionBehavior, params};
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

#[derive(Debug, PartialEq, Eq, Serialize)]
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
    pub summary: CommitReviewSummary,
    pub reviewed_files: Vec<usize>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitReviewProgress {
    pub summary: CommitReviewSummary,
    pub reviewed_files: Vec<usize>,
}

pub(crate) fn list(
    connection: &Connection,
    project_id: i64,
) -> Result<Vec<CommitReviewSummary>, String> {
    summaries(connection, project_id, None)
        .map_err(|error| format!("Cannot list saved reviews: {error}"))
}

fn summaries(
    connection: &Connection,
    project_id: i64,
    review_id: Option<i64>,
) -> rusqlite::Result<Vec<CommitReviewSummary>> {
    let mut query = connection.prepare(
        "SELECT r.id, r.base_label, r.target_label, r.start_commit, r.end_commit,
                COUNT(f.file_index), COUNT(*) FILTER (WHERE f.reviewed)
         FROM commit_reviews r LEFT JOIN commit_review_files f ON f.review_id = r.id
         WHERE r.project_id = ?1 AND (?2 IS NULL OR r.id = ?2)
         GROUP BY r.id ORDER BY r.id DESC",
    )?;
    query
        .query_map(params![project_id, review_id], |row| {
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
        .collect()
}

fn summary(
    connection: &Connection,
    project_id: i64,
    id: i64,
) -> rusqlite::Result<CommitReviewSummary> {
    summaries(connection, project_id, Some(id))?
        .pop()
        .ok_or(rusqlite::Error::QueryReturnedNoRows)
}

impl Application {
    pub fn save_commit_review(
        &self,
        checkout: &Path,
        comparison: &Comparison,
    ) -> Result<i64, String> {
        self.save_commit_review_with_layout(checkout, comparison, None)
    }

    pub fn save_commit_review_with_layout(
        &self,
        checkout: &Path,
        comparison: &Comparison,
        layout: Option<DiffLayout>,
    ) -> Result<i64, String> {
        self.save_commit_review_with_summary(checkout, comparison, layout)
            .map(|summary| summary.id)
    }

    pub fn save_commit_review_with_summary(
        &self,
        checkout: &Path,
        comparison: &Comparison,
        layout: Option<DiffLayout>,
    ) -> Result<CommitReviewSummary, String> {
        self.persist_commit_review(checkout, comparison, layout, None)
            .map(|progress| progress.summary)
    }

    pub fn start_commit_review(
        &self,
        checkout: &Path,
        comparison: &Comparison,
        layout: Option<DiffLayout>,
        file_index: usize,
    ) -> Result<CommitReviewProgress, String> {
        if file_index >= comparison.files.len() {
            return Err("Cannot start a review for a file outside the comparison.".into());
        }
        self.persist_commit_review(checkout, comparison, layout, Some(file_index))
    }

    pub fn find_commit_review(
        &self,
        checkout: &Path,
        comparison: &Comparison,
    ) -> Result<Option<CommitReviewProgress>, String> {
        let mut connection = self.connect()?;
        let project = project_id(&connection, checkout)?;
        let mut find = || {
            let transaction = connection.transaction()?;
            matching_review(&transaction, project, comparison)?
                .map(|id| review_progress(&transaction, project, id))
                .transpose()
        };
        find().map_err(|error| format!("Cannot find saved review: {error}"))
    }

    fn persist_commit_review(
        &self,
        checkout: &Path,
        comparison: &Comparison,
        layout: Option<DiffLayout>,
        first_mark: Option<usize>,
    ) -> Result<CommitReviewProgress, String> {
        let mut connection = self.connect()?;
        let project = project_id(&connection, checkout)?;
        let mut save = || {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            if let Some(index) = first_mark
                && let Some(id) = matching_review(&transaction, project, comparison)?
            {
                transaction.execute(
                    "UPDATE commit_review_files SET reviewed = 1 WHERE review_id = ?1 AND file_index = ?2",
                    params![id, index],
                )?;
                let progress = review_progress(&transaction, project, id)?;
                transaction.commit()?;
                return Ok(progress);
            }
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
                   (review_id, file_index, status, path, previous_path, patch, reviewed)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for (index, file) in comparison.files.iter().enumerate() {
                insert.execute(params![
                    id,
                    index,
                    file.status as u32,
                    file.path_bytes(),
                    file.previous_path_bytes(),
                    file.patch,
                    first_mark == Some(index),
                ])?;
            }
            drop(insert);
            if let Some(layout) = layout {
                transaction.execute(
                    "INSERT INTO commit_review_layouts (review_id, split) VALUES (?1, ?2)",
                    params![id, layout == DiffLayout::Split],
                )?;
            }
            let progress = review_progress(&transaction, project, id)?;
            transaction.commit()?;
            Ok::<_, rusqlite::Error>(progress)
        };
        save().map_err(|error| format!("Cannot save commit review: {error}"))
    }

    pub fn open_commit_review(&self, checkout: &Path, id: i64) -> Result<CommitReview, String> {
        let connection = self.connect()?;
        let project = project_id(&connection, checkout)?;
        let open = || {
            let transaction = connection.unchecked_transaction()?;
            let mut comparison = transaction.query_row(
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
            let mut query = transaction.prepare(
                "SELECT file_index, status, CAST(path AS BLOB), CAST(previous_path AS BLOB), patch, reviewed
                 FROM commit_review_files WHERE review_id = ?1 ORDER BY file_index",
            )?;
            let files = query.query_map([id], |row| {
                Ok((
                    row.get::<_, usize>(0)?,
                    row.get::<_, bool>(5)?,
                    ChangedFile::new(
                        char::from_u32(row.get(1)?).unwrap_or(char::REPLACEMENT_CHARACTER),
                        &row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, Option<Vec<u8>>>(3)?.as_deref(),
                        row.get(4)?,
                    ),
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
                summary: summary(&transaction, project, id)?,
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
        self.update_commit_file_review(checkout, id, file_index, reviewed)
            .map(|progress| progress.reviewed_files)
    }

    pub fn update_commit_file_review(
        &self,
        checkout: &Path,
        id: i64,
        file_index: usize,
        reviewed: bool,
    ) -> Result<CommitReviewProgress, String> {
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
            let progress = review_progress(&transaction, project, id)?;
            transaction.commit()?;
            Ok(progress)
        };
        update().map_err(|error| format!("Cannot update review mark: {error}"))
    }
}

fn review_progress(
    connection: &Connection,
    project: i64,
    id: i64,
) -> rusqlite::Result<CommitReviewProgress> {
    let mut query = connection.prepare(
        "SELECT file_index FROM commit_review_files WHERE review_id = ?1 AND reviewed ORDER BY file_index",
    )?;
    let reviewed_files = query
        .query_map([id], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(CommitReviewProgress {
        summary: summary(connection, project, id)?,
        reviewed_files,
    })
}

fn matching_review(
    connection: &Connection,
    project: i64,
    comparison: &Comparison,
) -> rusqlite::Result<Option<i64>> {
    let mut reviews = connection.prepare(
        "SELECT id FROM commit_reviews
         WHERE project_id = ?1 AND divergence = ?2 AND base_commit = ?3 AND start_commit = ?4 AND end_commit = ?5
           AND (target_label = 'Working tree') = ?6
         ORDER BY id DESC",
    )?;
    let mut files = connection.prepare(
        "SELECT status, CAST(path AS BLOB), CAST(previous_path AS BLOB), patch
         FROM commit_review_files WHERE review_id = ?1 ORDER BY file_index",
    )?;
    for id in reviews.query_map(
        params![
            project,
            comparison.mode == ComparisonMode::Divergence,
            comparison.base,
            comparison.start,
            comparison.end,
            comparison.target_label == "Working tree"
        ],
        |row| row.get::<_, i64>(0),
    )? {
        let id = id?;
        let mut rows = files.query([id])?;
        let mut matches = true;
        for file in &comparison.files {
            let Some(row) = rows.next()? else {
                matches = false;
                break;
            };
            if row.get::<_, u32>(0)? != file.status as u32
                || row.get::<_, Vec<u8>>(1)? != file.path_bytes()
                || row.get::<_, Option<Vec<u8>>>(2)?.as_deref() != file.previous_path_bytes()
                || row.get::<_, String>(3)? != file.patch
            {
                matches = false;
                break;
            }
        }
        if matches && rows.next()?.is_none() {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

pub(crate) fn project_id(connection: &Connection, checkout: &Path) -> Result<i64, String> {
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

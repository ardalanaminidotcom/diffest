use crate::{Application, git, path_text, reviews::project_id};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub(crate) const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS commit_review_layouts (
        review_id INTEGER PRIMARY KEY REFERENCES commit_reviews(id),
        split INTEGER NOT NULL CHECK (split IN (0, 1))
    );
    CREATE TABLE IF NOT EXISTS worktree_review_layouts (
        worktree TEXT PRIMARY KEY,
        split INTEGER NOT NULL CHECK (split IN (0, 1))
    );";

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffLayout {
    #[default]
    Unified,
    Split,
}

impl Application {
    pub fn read_review_layout(
        &self,
        checkout: &Path,
        id: Option<i64>,
    ) -> Result<Option<DiffLayout>, String> {
        let connection = self.connect()?;
        let split = if let Some(id) = id {
            let project = project_id(&connection, checkout)?;
            connection.query_row(
                "SELECT layout.split FROM commit_reviews review
                 LEFT JOIN commit_review_layouts layout ON layout.review_id = review.id
                 WHERE review.id = ?1 AND review.project_id = ?2",
                params![id, project],
                |row| row.get(0),
            )
        } else {
            let checkout = git::checkout_root(checkout)?;
            let directory = git::worktree_directory(&checkout)?;
            connection
                .query_row(
                    "SELECT split FROM worktree_review_layouts WHERE worktree = ?1",
                    [path_text(&directory)?],
                    |row| row.get(0),
                )
                .optional()
                .map(Option::flatten)
        };
        split
            .map(|split| {
                split.map(|split| {
                    if split {
                        DiffLayout::Split
                    } else {
                        DiffLayout::Unified
                    }
                })
            })
            .map_err(|error| format!("Cannot read review layout: {error}"))
    }

    pub fn set_review_layout(
        &self,
        checkout: &Path,
        id: Option<i64>,
        layout: DiffLayout,
    ) -> Result<(), String> {
        let connection = self.connect()?;
        let split = layout == DiffLayout::Split;
        let changed = if let Some(id) = id {
            let project = project_id(&connection, checkout)?;
            connection.execute(
                "INSERT INTO commit_review_layouts (review_id, split)
                 SELECT id, ?1 FROM commit_reviews WHERE id = ?2 AND project_id = ?3
                 ON CONFLICT (review_id) DO UPDATE SET split = excluded.split",
                params![split, id, project],
            )
        } else {
            let checkout = git::checkout_root(checkout)?;
            let directory = git::worktree_directory(&checkout)?;
            connection.execute(
                "INSERT INTO worktree_review_layouts (worktree, split) VALUES (?1, ?2)
                 ON CONFLICT (worktree) DO UPDATE SET split = excluded.split",
                params![path_text(&directory)?, split],
            )
        }
        .map_err(|error| format!("Cannot save review layout: {error}"))?;
        if changed == 0 {
            return Err("Cannot save review layout: review not found in this project".into());
        }
        Ok(())
    }
}

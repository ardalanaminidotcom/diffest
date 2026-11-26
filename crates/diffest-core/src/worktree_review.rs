use crate::{Application, ChangedFile, git, path_text};
use rusqlite::{TransactionBehavior, params};
use serde::Serialize;
use std::{collections::HashSet, path::Path};

pub(crate) const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS worktree_review_marks (
        worktree TEXT NOT NULL,
        fingerprint TEXT NOT NULL,
        PRIMARY KEY (worktree, fingerprint)
    );";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchSide {
    Staged,
    Unstaged,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchVersion {
    pub mode: String,
    // A worktree object ID identifies Git's diff input, which may not exist in the object store.
    pub object_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreePatch {
    #[serde(flatten)]
    pub file: ChangedFile,
    pub side: PatchSide,
    pub fingerprint: String,
    pub before: Option<PatchVersion>,
    pub after: Option<PatchVersion>,
    pub reviewed: bool,
}

impl Application {
    pub fn open_worktree_review(&self, checkout: &Path) -> Result<Vec<WorktreePatch>, String> {
        self.refresh_worktree_review(checkout, None)
    }

    pub fn set_worktree_patch_reviewed(
        &self,
        checkout: &Path,
        fingerprint: &str,
        reviewed: bool,
    ) -> Result<Vec<WorktreePatch>, String> {
        self.refresh_worktree_review(checkout, Some((fingerprint, reviewed)))
    }

    fn refresh_worktree_review(
        &self,
        checkout: &Path,
        mark: Option<(&str, bool)>,
    ) -> Result<Vec<WorktreePatch>, String> {
        let checkout = git::checkout_root(checkout)?;
        let directory = git::worktree_directory(&checkout)?;
        let worktree = path_text(&directory)?;
        let mut connection = self.connect()?;
        let error = |error| format!("Cannot update worktree review: {error}");
        // Serialize refreshes and marks so an older refresh cannot remove a newer mark.
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(error)?;
        let mut patches = git::uncommitted_patches(&checkout)?;
        let mut update = || {
            let mut marks = {
                let mut query = transaction
                    .prepare("SELECT fingerprint FROM worktree_review_marks WHERE worktree = ?1")?;
                query
                    .query_map([worktree], |row| row.get(0))?
                    .collect::<rusqlite::Result<HashSet<String>>>()?
            };
            let mut delete = transaction.prepare(
                "DELETE FROM worktree_review_marks WHERE worktree = ?1 AND fingerprint = ?2",
            )?;
            for patch in &mut patches {
                patch.reviewed = marks.remove(&patch.fingerprint);
                if let Some((fingerprint, reviewed)) = mark
                    && fingerprint == patch.fingerprint
                {
                    patch.reviewed = reviewed;
                    if reviewed {
                        transaction.execute(
                            "INSERT OR IGNORE INTO worktree_review_marks VALUES (?1, ?2)",
                            params![worktree, fingerprint],
                        )?;
                    } else {
                        delete.execute(params![worktree, fingerprint])?;
                    }
                }
            }
            for obsolete in marks {
                delete.execute(params![worktree, obsolete])?;
            }
            Ok::<_, rusqlite::Error>(())
        };
        update().map_err(error)?;
        transaction.commit().map_err(error)?;
        Ok(patches)
    }
}

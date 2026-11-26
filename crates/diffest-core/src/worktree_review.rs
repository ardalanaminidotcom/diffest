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

#[derive(Serialize)]
pub struct StageWorktreePatchResult {
    pub patches: Vec<WorktreePatch>,
    pub error: Option<String>,
}

enum ReviewAction<'a> {
    Refresh,
    Mark(&'a str, bool),
    Stage(&'a str),
}

impl Application {
    pub fn open_worktree_review(&self, checkout: &Path) -> Result<Vec<WorktreePatch>, String> {
        Ok(self
            .refresh_worktree_review(checkout, ReviewAction::Refresh)?
            .patches)
    }

    pub fn set_worktree_patch_reviewed(
        &self,
        checkout: &Path,
        fingerprint: &str,
        reviewed: bool,
    ) -> Result<Vec<WorktreePatch>, String> {
        Ok(self
            .refresh_worktree_review(checkout, ReviewAction::Mark(fingerprint, reviewed))?
            .patches)
    }

    pub fn stage_worktree_patch(
        &self,
        checkout: &Path,
        fingerprint: &str,
    ) -> Result<StageWorktreePatchResult, String> {
        self.refresh_worktree_review(checkout, ReviewAction::Stage(fingerprint))
    }

    fn refresh_worktree_review(
        &self,
        checkout: &Path,
        action: ReviewAction<'_>,
    ) -> Result<StageWorktreePatchResult, String> {
        let checkout = git::checkout_root(checkout)?;
        let directory = git::worktree_directory(&checkout)?;
        let worktree = path_text(&directory)?;
        let mut connection = self.connect()?;
        let error = |error| format!("Cannot update worktree review: {error}");
        // Serialize refreshes, marks and staging so an older refresh cannot remove a newer mark.
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(error)?;
        let mut marks = {
            let mut query = transaction
                .prepare("SELECT fingerprint FROM worktree_review_marks WHERE worktree = ?1")
                .map_err(error)?;
            query
                .query_map([worktree], |row| row.get(0))
                .map_err(error)?
                .collect::<rusqlite::Result<HashSet<String>>>()
                .map_err(error)?
        };
        let (staged_from, stage_error) = if let ReviewAction::Stage(fingerprint) = action {
            if marks.contains(fingerprint) {
                match git::stage_reviewed_patch(&checkout, &directory, fingerprint) {
                    Ok(patch) => (Some(patch), None),
                    Err(error) => (None, Some(error)),
                }
            } else {
                (
                    None,
                    Some("Only a current reviewed unstaged change can be staged.".into()),
                )
            }
        } else {
            (None, None)
        };
        let mut patches = git::uncommitted_patches(&checkout)?;
        let update = || {
            let mut delete = transaction.prepare(
                "DELETE FROM worktree_review_marks WHERE worktree = ?1 AND fingerprint = ?2",
            )?;
            for patch in &mut patches {
                let fingerprint = &patch.fingerprint;
                patch.reviewed = marks.remove(fingerprint);
                let mark = match action {
                    ReviewAction::Mark(marked, reviewed) if marked == fingerprint => Some(reviewed),
                    _ if staged_from
                        .as_ref()
                        .is_some_and(|reviewed| patch.matches_staged(reviewed)) =>
                    {
                        Some(true)
                    }
                    _ => None,
                };
                if let Some(reviewed) = mark {
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
        Ok(StageWorktreePatchResult {
            patches,
            error: stage_error,
        })
    }
}

impl WorktreePatch {
    fn matches_staged(&self, reviewed: &Self) -> bool {
        self.side == PatchSide::Staged
            && self.file.path_bytes() == reviewed.file.path_bytes()
            && self.file.previous_path_bytes() == reviewed.file.previous_path_bytes()
            && self.file.status == reviewed.file.status
            && self.before == reviewed.before
            && self.after == reviewed.after
    }
}

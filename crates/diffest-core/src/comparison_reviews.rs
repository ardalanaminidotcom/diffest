use crate::{
    Application, Comparison, ComparisonMode, ComparisonSession, DiffLayout, git, path_text,
    reviews::{CommitReviewProgress, project_id, read_review, review_progress, store_review},
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

pub(crate) const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS comparison_selections (
        review_id INTEGER PRIMARY KEY REFERENCES commit_reviews(id),
        project_id INTEGER NOT NULL REFERENCES projects(id),
        source TEXT NOT NULL,
        target TEXT NOT NULL,
        divergence INTEGER NOT NULL,
        worktree TEXT NOT NULL,
        last_viewed INTEGER NOT NULL DEFAULT (unixepoch()),
        branch_pair INTEGER GENERATED ALWAYS AS (
            (source GLOB 'refs/heads/*' OR source GLOB 'refs/remotes/*')
            AND (target GLOB 'refs/heads/*' OR target GLOB 'refs/remotes/*')
        ),
        UNIQUE (project_id, source, target, divergence, worktree)
    );
    CREATE TABLE IF NOT EXISTS review_retention (
        scope INTEGER PRIMARY KEY,
        days INTEGER NOT NULL CHECK (days BETWEEN 1 AND 4294967295)
    );
    CREATE TRIGGER IF NOT EXISTS discard_comparison_review BEFORE DELETE ON commit_reviews
    BEGIN
        DELETE FROM comparison_selections WHERE review_id = OLD.id;
        DELETE FROM commit_review_layouts WHERE review_id = OLD.id;
        DELETE FROM commit_review_files WHERE review_id = OLD.id;
    END;";

const WORKING_TREE: &str = "Working tree";
const DEFAULT_DAYS: u32 = 7;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewSelection {
    base: String,
    target: String,
    mode: ComparisonMode,
}

#[derive(PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonUpdate {
    Current,
    Available,
    BranchDeleted,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewRetention {
    pub days: u32,
    pub override_days: Option<u32>,
}

impl ReviewSelection {
    fn resolve(
        checkout: &Path,
        base: &str,
        target: &str,
        mode: ComparisonMode,
    ) -> Result<Self, String> {
        let endpoint = |revision| {
            let commit = git::resolve_commit(checkout, revision)?;
            let reference = git::git(
                checkout,
                &[
                    "rev-parse",
                    "--verify",
                    "--symbolic-full-name",
                    "--end-of-options",
                    revision,
                ],
            )
            .ok()
            .and_then(|output| String::from_utf8(output).ok());
            Ok::<_, String>(
                reference
                    .map(|name| name.trim_end_matches('\n').to_owned())
                    .filter(|name| is_branch(name))
                    .unwrap_or(commit),
            )
        };
        Ok(Self {
            base: endpoint(base)?,
            target: if target == WORKING_TREE {
                target.into()
            } else {
                endpoint(target)?
            },
            mode,
        })
    }

    fn worktree(&self, checkout: &Path) -> Result<String, String> {
        if self.target != WORKING_TREE {
            return Ok(String::new());
        }
        Ok(path_text(&git::worktree_directory(checkout)?)?.to_owned())
    }

    fn saved_id(
        &self,
        db: &Connection,
        project: i64,
        worktree: &str,
    ) -> rusqlite::Result<Option<i64>> {
        db.query_row(
            "SELECT review_id FROM comparison_selections
             WHERE project_id = ?1 AND source = ?2 AND target = ?3 AND divergence = ?4 AND worktree = ?5",
            params![project, self.base, self.target, self.mode == ComparisonMode::Divergence, worktree],
            |row| row.get(0),
        ).optional()
    }

    fn save(&self, db: &Connection, id: i64, project: i64, worktree: &str) -> rusqlite::Result<()> {
        db.execute(
            "INSERT INTO comparison_selections (review_id, project_id, source, target, divergence, worktree)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, project, self.base, self.target, self.mode == ComparisonMode::Divergence, worktree],
        )?;
        Ok(())
    }
}

impl Application {
    pub fn open_comparison(
        &self,
        checkout: &Path,
        base: &str,
        target: &str,
        mode: ComparisonMode,
        refresh: bool,
    ) -> Result<ComparisonSession, String> {
        let mut active = self
            .active_review
            .lock()
            .map_err(|error| error.to_string())?;
        let selection = ReviewSelection::resolve(checkout, base, target, mode)?;
        let worktree = selection.worktree(checkout)?;
        let mut connection = self.connect()?;
        let project = project_id(&connection, checkout)?;
        let tips = branch_tips(checkout)?;
        let open = || -> Result<ComparisonSession, String> {
            let db = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(storage_error)?;
            clear_expired(&db, *active).map_err(storage_error)?;
            clear_deleted_branches(&db, project, &tips).map_err(storage_error)?;
            let old_id = selection
                .saved_id(&db, project, &worktree)
                .map_err(storage_error)?;
            let saved = old_id
                .map(|id| read_review(&db, project, id))
                .transpose()
                .map_err(storage_error)?;
            let (mut comparison, progress) = match saved {
                Some(saved) if !refresh && worktree.is_empty() => {
                    // The stored patch remains readable even after a force-push.
                    (
                        saved.comparison,
                        Some(CommitReviewProgress {
                            summary: saved.summary,
                            reviewed_files: saved.reviewed_files,
                        }),
                    )
                }
                saved => {
                    let latest =
                        self.compare_revisions(checkout, &selection.base, &selection.target, mode)?;
                    let progress = if let Some(saved) = saved {
                        let unchanged: HashSet<_> = saved
                            .reviewed_files
                            .iter()
                            .map(|&index| file_identity(&saved.comparison.files[index]))
                            .collect();
                        let marks: Vec<_> = latest
                            .files
                            .iter()
                            .enumerate()
                            .filter_map(|(index, file)| {
                                unchanged.contains(&file_identity(file)).then_some(index)
                            })
                            .collect();
                        if same_snapshot(&latest, &saved.comparison) {
                            Some(CommitReviewProgress {
                                summary: saved.summary,
                                reviewed_files: saved.reviewed_files,
                            })
                        } else {
                            let id = saved.summary.id;
                            let layout = review_layout(&db, id).map_err(storage_error)?;
                            let replacement = if marks.is_empty() {
                                None
                            } else {
                                Some(
                                    store_review(&db, project, &latest, layout, &marks)
                                        .map_err(storage_error)?,
                                )
                            };
                            delete_review(&db, id).map_err(storage_error)?;
                            replacement
                                .map(|id| {
                                    selection.save(&db, id, project, &worktree)?;
                                    review_progress(&db, project, id)
                                })
                                .transpose()
                                .map_err(storage_error)?
                        }
                    } else {
                        None
                    };
                    (latest, progress)
                }
            };
            comparison.base_label = base.to_owned();
            comparison.target_label = target.to_owned();
            let next = progress.as_ref().map(|progress| progress.summary.id);
            if *active != next {
                finish_review(&db, *active).map_err(storage_error)?;
            }
            touch_review(&db, next).map_err(storage_error)?;
            let update = update_status(&selection, &tips, &comparison.base, &comparison.end);
            let layout = next
                .map(|id| review_layout(&db, id))
                .transpose()
                .map_err(storage_error)?
                .flatten();
            db.commit().map_err(storage_error)?;
            *active = next;
            Ok(ComparisonSession {
                comparison,
                progress,
                selection,
                update,
                layout,
            })
        };
        open()
    }

    pub fn start_comparison_review(
        &self,
        checkout: &Path,
        selection: &ReviewSelection,
        comparison: &Comparison,
        layout: Option<DiffLayout>,
        file_index: usize,
    ) -> Result<CommitReviewProgress, String> {
        if file_index >= comparison.files.len() || selection.mode != comparison.mode {
            return Err("Cannot mark a file outside the active comparison.".into());
        }
        let mut active = self
            .active_review
            .lock()
            .map_err(|error| error.to_string())?;
        let tips = branch_tips(checkout)?;
        if update_status(selection, &tips, &comparison.base, &comparison.end)
            == ComparisonUpdate::BranchDeleted
        {
            return Err("A branch in this comparison was deleted.".into());
        }
        let worktree = selection.worktree(checkout)?;
        let mut db = self.connect()?;
        let project = project_id(&db, checkout)?;
        let db = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let id = if let Some(id) = selection
            .saved_id(&db, project, &worktree)
            .map_err(storage_error)?
        {
            let saved = read_review(&db, project, id).map_err(storage_error)?;
            if !same_snapshot(&saved.comparison, comparison) {
                return Err("The comparison has changed. Reopen it before marking files.".into());
            }
            mark_file(&db, id, file_index, true).map_err(storage_error)?;
            id
        } else {
            let id = store_review(&db, project, comparison, layout, &[file_index])
                .map_err(storage_error)?;
            selection
                .save(&db, id, project, &worktree)
                .map_err(storage_error)?;
            id
        };
        if *active != Some(id) {
            finish_review(&db, *active).map_err(storage_error)?;
        }
        touch_review(&db, Some(id)).map_err(storage_error)?;
        let progress = review_progress(&db, project, id).map_err(storage_error)?;
        db.commit().map_err(storage_error)?;
        *active = Some(id);
        Ok(progress)
    }

    pub fn mark_comparison_file(
        &self,
        checkout: &Path,
        id: i64,
        file_index: usize,
        reviewed: bool,
    ) -> Result<Option<CommitReviewProgress>, String> {
        let mut active = self
            .active_review
            .lock()
            .map_err(|error| error.to_string())?;
        if *active != Some(id) {
            return Err("This review is no longer active.".into());
        }
        let mut db = self.connect()?;
        let project = project_id(&db, checkout)?;
        let tips = branch_tips(checkout)?;
        let db = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        clear_deleted_branches(&db, project, &tips).map_err(storage_error)?;
        let exists = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM comparison_selections WHERE review_id = ?1 AND project_id = ?2)",
            params![id, project],
            |row| row.get::<_, bool>(0),
        ).map_err(storage_error)?;
        if !exists {
            db.commit().map_err(storage_error)?;
            return Err("This review is no longer available.".into());
        }
        mark_file(&db, id, file_index, reviewed).map_err(storage_error)?;
        let progress = review_progress(&db, project, id).map_err(storage_error)?;
        let next = (!progress.reviewed_files.is_empty()).then_some(progress);
        if next.is_none() {
            delete_review(&db, id).map_err(storage_error)?;
        } else {
            touch_review(&db, Some(id)).map_err(storage_error)?;
        }
        db.commit().map_err(storage_error)?;
        *active = next.as_ref().map(|progress| progress.summary.id);
        Ok(next)
    }

    pub fn leave_comparison_review(&self) -> Result<(), String> {
        let mut active = self
            .active_review
            .lock()
            .map_err(|error| error.to_string())?;
        if active.is_none() {
            return Ok(());
        }
        finish_review(&self.connect()?, *active).map_err(storage_error)?;
        *active = None;
        Ok(())
    }

    pub fn comparison_update(
        &self,
        checkout: &Path,
        selection: &ReviewSelection,
        base: &str,
        end: &str,
    ) -> Result<ComparisonUpdate, String> {
        let active = self
            .active_review
            .lock()
            .map_err(|error| error.to_string())?;
        let tips = branch_tips(checkout)?;
        let db = self.connect()?;
        let project = project_id(&db, checkout)?;
        clear_deleted_branches(&db, project, &tips).map_err(storage_error)?;
        touch_review(&db, *active).map_err(storage_error)?;
        clear_expired(&db, *active).map_err(storage_error)?;
        Ok(update_status(selection, &tips, base, end))
    }

    pub fn check_deleted_reviews(&self, checkout: &Path) -> Result<(), String> {
        let _active = self
            .active_review
            .lock()
            .map_err(|error| error.to_string())?;
        let db = self.connect()?;
        let project = project_id(&db, checkout)?;
        let tips = branch_tips(checkout)?;
        clear_deleted_branches(&db, project, &tips).map_err(storage_error)
    }

    pub fn clean_comparison_reviews(&self) -> Result<(), String> {
        let active = self
            .active_review
            .lock()
            .map_err(|error| error.to_string())?;
        let db = self.connect()?;
        db.execute(
            "DELETE FROM commit_reviews WHERE id NOT IN (SELECT review_id FROM comparison_selections)",
            [],
        ).map_err(storage_error)?;
        clear_expired(&db, *active).map_err(storage_error)?;
        let mut query = db
            .prepare(
                "SELECT id, main_checkout, common_directory FROM projects
             WHERE id IN (SELECT project_id FROM comparison_selections)",
            )
            .map_err(storage_error)?;
        let projects = query
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(storage_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(storage_error)?;
        for (project, checkout, common) in projects {
            let checkout = Path::new(&checkout);
            if git::common_directory(checkout)
                .is_ok_and(|directory| directory == Path::new(&common))
                && let Ok(tips) = branch_tips(checkout)
            {
                clear_deleted_branches(&db, project, &tips).map_err(storage_error)?;
            }
        }
        Ok(())
    }

    pub fn review_retention(&self, checkout: Option<&Path>) -> Result<ReviewRetention, String> {
        let db = self.connect()?;
        let scope = checkout
            .map(|path| project_id(&db, path))
            .transpose()?
            .unwrap_or(0);
        retention(&db, scope).map_err(storage_error)
    }

    pub fn save_review_retention(
        &self,
        checkout: Option<&Path>,
        days: Option<u32>,
    ) -> Result<ReviewRetention, String> {
        let active = self
            .active_review
            .lock()
            .map_err(|error| error.to_string())?;
        if days == Some(0) || (checkout.is_none() && days.is_none()) {
            return Err("Enter a positive number of days.".into());
        }
        let mut db = self.connect()?;
        let scope = checkout
            .map(|path| project_id(&db, path))
            .transpose()?
            .unwrap_or(0);
        let db = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        if let Some(days) = days {
            db.execute(
                "INSERT INTO review_retention VALUES (?1, ?2)
                 ON CONFLICT (scope) DO UPDATE SET days = excluded.days",
                params![scope, days],
            )
            .map_err(storage_error)?;
        } else {
            db.execute("DELETE FROM review_retention WHERE scope = ?1", [scope])
                .map_err(storage_error)?;
        }
        clear_expired(&db, *active).map_err(storage_error)?;
        let saved = retention(&db, scope).map_err(storage_error)?;
        db.commit().map_err(storage_error)?;
        Ok(saved)
    }
}

fn review_layout(db: &Connection, id: i64) -> rusqlite::Result<Option<DiffLayout>> {
    db.query_row(
        "SELECT split FROM commit_review_layouts WHERE review_id = ?1",
        [id],
        |row| {
            Ok(if row.get::<_, bool>(0)? {
                DiffLayout::Split
            } else {
                DiffLayout::Unified
            })
        },
    )
    .optional()
}

fn storage_error(error: rusqlite::Error) -> String {
    format!("Cannot manage review progress: {error}")
}

fn retention(db: &Connection, scope: i64) -> rusqlite::Result<ReviewRetention> {
    let configured = |scope| {
        db.query_row(
            "SELECT days FROM review_retention WHERE scope = ?1",
            [scope],
            |row| row.get::<_, u32>(0),
        )
        .optional()
    };
    let override_days = configured(scope)?;
    let days = override_days.or(configured(0)?).unwrap_or(DEFAULT_DAYS);
    Ok(ReviewRetention {
        days,
        override_days,
    })
}

fn is_branch(reference: &str) -> bool {
    reference.starts_with("refs/heads/") || reference.starts_with("refs/remotes/")
}

struct BranchTips {
    refs: HashMap<String, String>,
    complete: bool,
}

fn branch_tips(checkout: &Path) -> Result<BranchTips, String> {
    let (output, diagnostics) = git::git_with_diagnostics(
        checkout,
        &[
            "for-each-ref",
            "--format=%(refname)%09%(objectname)",
            "refs/heads/",
            "refs/remotes/",
        ],
    )?;
    let refs = String::from_utf8_lossy(&output)
        .lines()
        .filter_map(|line| {
            let (reference, tip) = line.split_once('\t')?;
            Some((reference.to_owned(), tip.to_owned()))
        })
        .collect();
    // Git can warn about a broken ref and omit it while still exiting successfully.
    Ok(BranchTips {
        refs,
        complete: diagnostics.is_empty(),
    })
}

fn update_status(
    selection: &ReviewSelection,
    tips: &BranchTips,
    base: &str,
    end: &str,
) -> ComparisonUpdate {
    let mut moved = false;
    for (reference, pinned) in [(&selection.base, base), (&selection.target, end)] {
        if is_branch(reference) {
            match tips.refs.get(reference) {
                None if tips.complete => return ComparisonUpdate::BranchDeleted,
                None => {}
                Some(tip) => moved |= tip != pinned,
            }
        }
    }
    if moved {
        ComparisonUpdate::Available
    } else {
        ComparisonUpdate::Current
    }
}

fn clear_deleted_branches(
    db: &Connection,
    project: i64,
    tips: &BranchTips,
) -> rusqlite::Result<()> {
    if !tips.complete {
        return Ok(());
    }
    let mut query = db.prepare(
        "SELECT review_id, source, target FROM comparison_selections WHERE project_id = ?1",
    )?;
    let selections = query
        .query_map([project], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, base, target) in selections {
        if [base, target]
            .iter()
            .any(|endpoint| is_branch(endpoint) && !tips.refs.contains_key(endpoint))
        {
            delete_review(db, id)?;
        }
    }
    Ok(())
}

fn clear_expired(db: &Connection, active: Option<i64>) -> rusqlite::Result<()> {
    db.execute(
        "DELETE FROM commit_reviews WHERE id IN (
            SELECT s.review_id FROM comparison_selections s
            LEFT JOIN review_retention p ON p.scope = s.project_id
            LEFT JOIN review_retention g ON g.scope = 0
            WHERE s.review_id IS NOT ?1
              AND NOT s.branch_pair
              AND s.last_viewed <= unixepoch() - COALESCE(p.days, g.days, ?2) * 86400
        )",
        params![active, DEFAULT_DAYS],
    )?;
    Ok(())
}

fn finish_review(db: &Connection, id: Option<i64>) -> rusqlite::Result<()> {
    db.execute(
        "DELETE FROM commit_reviews WHERE id = ?1 AND id IN (
            SELECT review_id FROM comparison_selections WHERE NOT branch_pair
        ) AND NOT EXISTS (
            SELECT 1 FROM commit_review_files WHERE review_id = ?1 AND NOT reviewed
        )",
        [id],
    )?;
    touch_review(db, id)
}

fn touch_review(db: &Connection, id: Option<i64>) -> rusqlite::Result<()> {
    if let Some(id) = id {
        db.execute(
            "UPDATE comparison_selections SET last_viewed = unixepoch() WHERE review_id = ?1",
            [id],
        )?;
    }
    Ok(())
}

fn delete_review(db: &Connection, id: i64) -> rusqlite::Result<()> {
    db.execute("DELETE FROM commit_reviews WHERE id = ?1", [id])?;
    Ok(())
}

fn mark_file(db: &Connection, id: i64, index: usize, reviewed: bool) -> rusqlite::Result<()> {
    let changed = db.execute(
        "UPDATE commit_review_files SET reviewed = ?1 WHERE review_id = ?2 AND file_index = ?3",
        params![reviewed, id, index],
    )?;
    if changed == 0 {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }
    Ok(())
}

fn same_snapshot(first: &Comparison, second: &Comparison) -> bool {
    first.base == second.base
        && first.start == second.start
        && first.end == second.end
        && first
            .files
            .iter()
            .map(file_identity)
            .eq(second.files.iter().map(file_identity))
}

fn file_identity(file: &crate::ChangedFile) -> (char, &[u8], Option<&[u8]>, &str) {
    (
        file.status,
        file.path_bytes(),
        file.previous_path_bytes(),
        &file.patch,
    )
}

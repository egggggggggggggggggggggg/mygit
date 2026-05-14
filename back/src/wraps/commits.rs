//! Commit history inspection — streaming walks, range support, rich metadata.
//!
//! # Performance notes
//!
//! * The revision walker uses the **commit-graph file**
//!   (`.git/objects/info/commit-graph`) when present, giving O(1) parent
//!   lookups and generation-number queries without decoding full commit objects.
//!   Only the call to `repo.find_commit(id)` below pays the full decode cost.
//!
//! * `gix::Commit` decodes its raw bytes **lazily** on first field access and
//!   caches the result for the lifetime of the object.  Calling `.author()` and
//!   `.committer()` in the same `TryFrom` invocation triggers a single decode.
//!
//! * Pass `limit = Some(n)` to cap the walk early; the iterator is dropped
//!   without visiting the rest of the graph.

use anyhow::Context as _;
use gix::{ObjectId, Repository};
use serde::Serialize;
use time::OffsetDateTime;
use utoipa::ToSchema;

/// Full metadata extracted from a single commit.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CommitInfo {
    /// Full 40-hex SHA-1 / SHA-256 object ID.
    pub hash: String,

    /// Author display name (UTF-8, lossy).
    pub author_name: String,
    /// Author e-mail address.
    pub author_email: String,
    /// When the change was originally authored.
    pub authored_at: OffsetDateTime,

    /// Committer display name (differs from author on rebases / merges).
    pub committer_name: String,
    /// When the commit object was written (used for branch ordering).
    pub committed_at: OffsetDateTime,
    /// First line of the commit message (the "subject").
    pub summary: String,
    /// Remainder of the commit message after the blank line, if any.
    pub body: Option<String>,
}

impl<'repo> TryFrom<gix::Commit<'repo>> for CommitInfo {
    type Error = anyhow::Error;

    fn try_from(commit: gix::Commit<'repo>) -> Result<Self, Self::Error> {
        let author = commit.author().context("commit has no author signature")?;
        let committer = commit
            .committer()
            .context("commit has no committer signature")?;
        let message = commit
            .message()
            .context("commit message is not valid UTF-8")?;

        // convert gix::ObjectId -> hex string
        let hash = commit.id().to_hex().to_string();
        // convert gix::date::Time -> OffsetDateTime
        let authored_at = {
            let t = author.time()?;
            // gix::date::Time exposes `seconds`+`offset` or similar; use RFC3339 conversion
            OffsetDateTime::from_unix_timestamp(t.seconds)
                .map_err(|e| anyhow::anyhow!(e))?
                .to_offset(time::UtcOffset::from_whole_seconds(t.offset)?)
        };
        let committed_at = {
            let t = committer.time()?;
            OffsetDateTime::from_unix_timestamp(t.seconds)
                .map_err(|e| anyhow::anyhow!(e))?
                .to_offset(time::UtcOffset::from_whole_seconds(t.offset)?)
        };
        Ok(Self {
            hash,
            author_name: author.name.to_string(),
            author_email: author.email.to_string(),
            authored_at,
            committer_name: committer.name.to_string(),
            committed_at,
            summary: message.title.to_string(),
            body: message.body.map(|b| b.to_string()),
        })
    }
}

/// Return commits reachable from `branch`, newest first.
///
/// `branch` may be a short name (`"main"`), a remote-tracking name
/// (`"origin/main"`), or any revision expression accepted by `gix`.
///
/// Set `limit` to avoid walking the entire history when you only need recent
/// commits (e.g. a GitHub-style "last 30 commits" view).
pub fn commits_for_branch(
    repo: &Repository,
    branch: &str,
    limit: Option<usize>,
) -> Result<Vec<CommitInfo>, anyhow::Error> {
    let tip = repo
        .find_reference(branch)
        .with_context(|| format!("reference '{branch}' not found"))?
        // Peel annotated tags, symbolic refs, etc. down to a plain commit ID.
        .into_fully_peeled_id()
        .with_context(|| format!("could not peel '{branch}' to a commit"))?;

    walk_and_collect(repo, [tip.detach()], None, limit)
}

/// Return commits in the range `from..to` (exclusive of `from`, inclusive of
/// `to`), newest first.
///
/// Equivalent to `git log <from>..<to>`.
///
/// # Performance
/// The walk stops as soon as the `from` commit is reached, so it only traverses
/// the commits *between* the two points, not the full history.
pub fn commits_in_range(
    repo: &Repository,
    from: ObjectId,
    to: ObjectId,
    limit: Option<usize>,
) -> Result<Vec<CommitInfo>, anyhow::Error> {
    walk_and_collect(repo, [to], Some(from), limit)
}

/// Drive a revision walk from `tips`, stopping at (but not including)
/// `stop_at`, and convert each visited commit into [`CommitInfo`].
fn walk_and_collect(
    repo: &Repository,
    tips: impl IntoIterator<Item = ObjectId>,
    stop_at: Option<ObjectId>,
    limit: Option<usize>,
) -> Result<Vec<CommitInfo>, anyhow::Error> {
    // `rev_walk` returns a lazy platform; `.all()` starts the DFS/BFS walk.
    // The walk reads only commit-graph entries while traversing; full object
    // decoding happens only in `repo.find_commit()` below.
    let walk = repo
        .rev_walk(tips)
        .all()
        .context("failed to initialise revision walk")?;

    let cap = limit.unwrap_or(256);
    let mut result = Vec::with_capacity(cap.min(4096));

    for info in walk {
        if let Some(n) = limit
            && result.len() >= n
        {
            break;
        }

        let info = info.context("error advancing revision walk")?;

        // Stop before the exclusive boundary commit (implements `from..to`).
        if stop_at.is_some_and(|stop| stop == info.id) {
            break;
        }

        // Only here do we pay the cost of inflating and decoding the commit.
        let commit = repo
            .find_commit(info.id)
            .with_context(|| format!("could not load commit {}", info.id))?;

        result.push(CommitInfo::try_from(commit)?);
    }

    Ok(result)
}

pub fn commits_for_branch_paginated(
    repo: &Repository,
    branch: &str,
    page: usize,
    per_page: usize,
) -> Result<Vec<CommitInfo>, anyhow::Error> {
    let tip = repo
        .find_reference(branch)
        .with_context(|| format!("reference '{branch}' not found"))?
        .into_fully_peeled_id()
        .with_context(|| format!("could not peel '{branch}' to a commit"))?
        .detach();

    let skip = page.saturating_sub(1) * per_page;

    let walk = repo
        .rev_walk([tip])
        .all()
        .context("failed to initialise revision walk")?;

    let mut result = Vec::with_capacity(per_page);

    for (i, info) in walk.enumerate() {
        let info = info.context("error advancing revision walk")?;

        // Skip earlier pages
        if i < skip {
            continue;
        }

        // Stop once we filled the page
        if result.len() >= per_page {
            break;
        }

        let commit = repo
            .find_commit(info.id)
            .with_context(|| format!("could not load commit {}", info.id))?;

        result.push(CommitInfo::try_from(commit)?);
    }

    Ok(result)
}

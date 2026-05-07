//! Branch listing — local and remote, with tip commit IDs.
//!
//! # Performance note
//! `repo.references()` reads the packed-refs file *once* and streams loose
//! refs without buffering all of them in memory.  We never peel tag chains
//! unless the reference is a tag (branches always point directly to commits).

use anyhow::Context as _;
use gix::{ObjectId, Repository, bstr::ByteSlice, refs::Category};

/// A local branch (e.g. `main`, `feature/auth`).
#[derive(Debug, Clone)]
pub struct LocalBranch {
    /// Short name as shown by `git branch` (e.g. `"main"`).
    pub name: String,
    /// Commit the branch tip points to.
    pub tip: ObjectId,
}

/// A remote-tracking branch (e.g. `origin/main`).
#[derive(Debug, Clone)]
pub struct RemoteBranch {
    /// Remote name (e.g. `"origin"`).
    pub remote: String,
    /// Branch name on the remote (e.g. `"main"`).
    pub branch: String,
    /// Commit the remote-tracking tip points to.
    pub tip: ObjectId,
}

/// All branches in the repository, grouped by locality.
#[derive(Debug, Default)]
pub struct BranchListing {
    pub local: Vec<LocalBranch>,
    pub remote: Vec<RemoteBranch>,
}

//This method can lose data, might wanna rewrite this or at least indicate to the user stuff was
//lost. If we wanna scale this up, eg when we have distributed copies of data across multiple
//database we could use the metric of invalid references to try and fix broken data.
pub fn list_branches(repo: &Repository) -> Result<BranchListing, anyhow::Error> {
    let mut listing = BranchListing::default();

    for r in repo
        .references()
        .context("failed to open references iterator")?
        .all()
        .context("failed to iterate references")?
    {
        let r = match r {
            Ok(r) => r,
            Err(_) => {
                //Return some info regarding why this reference could not be used.
                continue;
            }
        };

        match r.name().category() {
            Some(Category::LocalBranch) => {
                listing.local.push(LocalBranch {
                    name: r.name().shorten().to_str_lossy().into_owned(),
                    tip: r.id().detach(),
                });
            }

            Some(Category::RemoteBranch) => {
                let short = r.name().shorten();
                let (remote, branch) = split_remote_branch(short);

                listing.remote.push(RemoteBranch {
                    remote,
                    branch,
                    tip: r.id().detach(),
                });
            }

            _ => {}
        }
    }
    Ok(listing)
}

/// Split `"origin/main"` → `("origin", "main")`.
/// Branch names can themselves contain `/`, so we split only on the *first* one.
fn split_remote_branch(short: &gix::bstr::BStr) -> (String, String) {
    short.find_byte(b'/').map_or_else(
        || (short.to_str_lossy().into_owned(), String::new()),
        |i| {
            (
                short[..i].to_str_lossy().into_owned(),
                short[i + 1..].to_str_lossy().into_owned(),
            )
        },
    )
}

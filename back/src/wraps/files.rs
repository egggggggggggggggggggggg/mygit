//! File reading at a specific commit snapshot.
//!
//! # Performance notes
//!
//! * Tree traversal is **path-depth-bounded**, not full-tree.  For a path
//!   `a/b/c.rs` we load exactly three tree objects (`a/`, `a/b/`, then the
//!   blob entry) regardless of how wide the tree is.
//!
//! * With the object cache enabled on the repository (see [`crate::open_repo`]),
//!   repeatedly reading files at the *same* commit reuses the cached root-tree
//!   object instead of re-inflating it from the pack.
//!
//! * We call `entry.object()` rather than `repo.find_object(entry.oid())`
//!   because the former can skip a round-trip when `gix` already holds the
//!   blob in a lookaside buffer from the tree walk.

use anyhow::{Context as _, bail};
use gix::{
    ObjectId, Repository,
    bstr::{BStr, ByteSlice},
    objs::tree::EntryKind,
};
use serde::Serialize;
use utoipa::ToSchema;

use crate::wraps::GixError;

// ── Public API ────────────────────────────────────────────────────────────────

/// Read the raw bytes of the file at `path` as it existed at `commit_id`.
///
/// `path` uses forward slashes and must be relative to the repository root
/// (e.g. `"src/main.rs"`, `"README.md"`).
///
/// # Errors
/// Returns an error if:
/// * `commit_id` does not exist in the repository.
/// * `path` does not exist in the commit's tree.
/// * `path` points to a directory, symlink, or submodule rather than a file.
pub fn read_file_at_commit(
    repo: &Repository,
    commit_id: ObjectId,
    path: &str,
) -> Result<Vec<u8>, anyhow::Error> {
    let path_bstr: &BStr = path.as_bytes().as_bstr();

    let commit = repo
        .find_commit(commit_id)
        .with_context(|| format!("commit {commit_id} not found"))?;

    // Loading the tree only reads the *root* tree object header.  Subtree
    // objects for nested path components are fetched on demand below.
    let tree = commit.tree().context("failed to read commit tree")?;

    let entry = tree
        .lookup_entry_by_path(path_bstr.to_string())
        .with_context(|| format!("tree traversal failed for '{path}'"))?
        .with_context(|| format!("'{path}' does not exist at commit {commit_id}"))?;

    // Reject anything that is not a plain file — calling code should not have
    // to guess whether the returned bytes are a tree listing or a symlink target.
    match entry.mode().kind() {
        EntryKind::Blob | EntryKind::BlobExecutable => {}
        EntryKind::Tree => bail!("'{path}' is a directory, not a file"),
        EntryKind::Link => bail!("'{path}' is a symbolic link"),
        EntryKind::Commit => bail!("'{path}' is a submodule"),
    }

    // `object()` fetches the blob from the ODB (or the object cache if warm).
    // We clone the data out so the caller owns it without lifetime coupling to
    // the commit/tree stack.
    let blob = entry
        .object()
        .context("failed to load blob object")?
        .try_into_blob()
        .context("expected a blob object")?;

    Ok(blob.data.to_vec())
}

/// Like [`read_file_at_commit`] but looks up the commit via a branch name or
/// any other revision expression (e.g. `"HEAD"`, `"v1.2.3"`, `"main~5"`).
///
/// Useful when the caller already has a revision string rather than an ID.
pub fn read_file_at_rev(
    repo: &Repository,
    rev: &str,
    path: &str,
) -> Result<Vec<u8>, anyhow::Error> {
    let id = repo
        .rev_parse_single(rev)
        .with_context(|| format!("could not resolve revision '{rev}'"))?
        .detach();

    read_file_at_commit(repo, id, path)
}

/// Return `true` if `path` exists as a regular file at `commit_id`, without
/// loading the blob contents.
///
/// Useful for existence checks that should avoid the cost of reading large files.
pub fn file_exists_at_commit(
    repo: &Repository,
    commit_id: ObjectId,
    path: &str,
) -> Result<bool, anyhow::Error> {
    let path_bstr: &BStr = path.as_bytes().as_bstr();

    let commit = repo
        .find_commit(commit_id)
        .with_context(|| format!("commit {commit_id} not found"))?;
    let tree = commit.tree().context("failed to read commit tree")?;

    let entry = tree
        .lookup_entry_by_path(path_bstr.to_string())
        .with_context(|| format!("tree traversal failed for '{path}'"))?;

    Ok(entry
        .is_some_and(|e| matches!(e.mode().kind(), EntryKind::Blob | EntryKind::BlobExecutable)))
}
#[derive(Debug, Serialize, ToSchema)]
#[serde(tag = "type")]
pub enum Node {
    File {
        name: String,
        oid: String,
    },
    #[schema(no_recursion)]
    Directory {
        name: String,
        children: Vec<Self>,
    },
    Submodule {
        name: String,
        oid: String,
    },
}
pub fn get_tree(repo: &Repository, commit_id: ObjectId, name: &str) -> Result<Node, GixError> {
    let commit = repo.find_commit(commit_id)?;
    let tree = commit.tree()?;
    build_tree(repo, &tree, name)
}
pub fn build_tree(repo: &gix::Repository, tree: &gix::Tree, name: &str) -> Result<Node, GixError> {
    let mut children = Vec::new();
    for entry in tree.iter() {
        let entry = entry.map_err(|_| GixError::Unimplemented)?;
        let entry_name = entry.filename().to_string();

        match entry.mode().kind() {
            EntryKind::Blob => {
                children.push(Node::File {
                    name: entry_name,
                    oid: entry.oid().to_string(),
                });
            }
            EntryKind::Tree => {
                let subtree = repo.find_object(entry.oid())?.into_tree();

                let node = build_tree(repo, &subtree, &entry_name)?;

                children.push(node);
            }
            EntryKind::Commit => {
                children.push(Node::Submodule {
                    name: entry_name,
                    oid: entry.oid().to_string(),
                });
            }

            _ => {}
        }
    }
    Ok(Node::Directory {
        name: name.to_string(),
        children,
    })
}
#[derive(Debug)]
pub struct BlobEntry {
    pub path: String,
    pub oid: String,
}

pub fn collect_blobs(
    repo: &gix::Repository,
    tree: &gix::Tree,
    prefix: &str,
    out: &mut Vec<BlobEntry>,
) -> anyhow::Result<()> {
    for entry in tree.iter() {
        let entry = entry?;
        let name = entry.filename().to_string();

        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{}/{}", prefix, name)
        };
        match entry.mode().kind() {
            EntryKind::Blob => {
                out.push(BlobEntry {
                    path,
                    oid: entry.oid().to_string(),
                });
            }
            EntryKind::Tree => {
                let subtree = repo.find_object(entry.oid())?.into_tree();

                collect_blobs(repo, &subtree, &path, out)?;
            }
            EntryKind::Commit => {
                // submodule
            }

            _ => {}
        }
    }

    Ok(())
}

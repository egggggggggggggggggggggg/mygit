//idk what to name this, but its mainly just wrappers around gix calls
//A lot of methods panic when they should be recoverable.
pub mod branches;
pub mod commits;
pub mod files;
pub mod meta;
pub use branches::{BranchListing, LocalBranch, RemoteBranch, list_branches};
pub use commits::{CommitInfo, commits_for_branch, commits_in_range};
pub use files::read_file_at_commit;
use thiserror::Error;
#[derive(Debug, Error)]

///Gix genuinely has the worst error handling of any library crate. wtf is this.
pub enum GixError {
    #[error("failed to open repository")]
    OpenRepo,

    #[error("failed to read HEAD")]
    ReadHead(#[from] gix::reference::find::existing::Error),

    #[error("failed to resolve revision")]
    ResolveRev(#[from] gix::revision::spec::parse::single::Error),

    #[error("failed to peel to commit")]
    PeelCommit(#[from] gix::head::peel::to_commit::Error),

    #[error("failed to peel to object")]
    PeelObject(#[from] gix::head::peel::to_object::Error),

    #[error("failed to acquire specified object")]
    AcquireObject(#[from] gix::object::commit::Error),

    #[error("failed to lookup")]
    Lookup(#[from] gix::objs::find::existing::Error),

    #[error("item does not exist")]
    MissingItem,

    #[error("temp")]
    MissingTest(#[from] gix::object::find::existing::with_conversion::Error),
    #[error("unimplemented cause theres too many arbirtrary errors")]
    Unimplemented,
}

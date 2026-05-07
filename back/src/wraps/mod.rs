//idk what to name this, but its mainly just wrappers around gix calls
//A lot of methods panic when they should be recoverable.
pub mod branches;
pub mod commits;
pub mod files;

pub use branches::{BranchListing, LocalBranch, RemoteBranch, list_branches};
pub use commits::{CommitInfo, commits_for_branch, commits_in_range};
pub use files::read_file_at_commit;

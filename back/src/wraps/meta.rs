use anyhow::Context;
use gix::{Repository, refs::Category};

pub fn commit_count_from_ref(repo: &Repository, rev: &str) -> Result<usize, anyhow::Error> {
    let tip = repo
        .rev_parse_single(rev)
        .with_context(|| format!("could not resolve '{rev}'"))?
        .detach();

    let walk = repo
        .rev_walk([tip])
        .all()
        .context("failed to start revision walk")?;

    let mut count = 0usize;

    for item in walk {
        item.context("rev walk failed")?;
        count += 1;
    }

    Ok(count)
}
pub fn total_commit_count(repo: &Repository) -> Result<usize, anyhow::Error> {
    let mut seen = std::collections::HashSet::new();

    for head in repo.references()?.all()? {
        let head = head.unwrap();
        let id = head.id().detach();
        let walk = repo.rev_walk([id]).all()?;

        for item in walk {
            let item = item?;
            seen.insert(item.id);
        }
    }

    Ok(seen.len())
}
pub struct RefData {
    pub branches: usize,
    pub tags: usize,
}

pub fn count_ref_types(repo: &gix::Repository) -> Result<RefData, anyhow::Error> {
    let mut branches = 0usize;
    let mut tags = 0usize;
    for reference in repo.references()?.all()? {
        let reference = match reference {
            Ok(r) => r,
            Err(_) => continue,
        };
        match reference.name().category() {
            None => {
                continue;
            }
            Some(a) => match a {
                Category::LocalBranch | Category::RemoteBranch => {
                    branches += 1;
                }
                Category::Tag => {
                    tags += 1;
                }
                _ => {}
            },
        }
    }
    Ok(RefData { branches, tags })
}

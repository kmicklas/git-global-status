use std::{
    borrow::Borrow,
    env::current_dir,
    fs::read_dir,
    io::{self, stdout},
    path::PathBuf,
};

#[derive(Default)]
struct Status {
    path: PathBuf,
    dirty: bool,
    untracked_branches: Vec<String>,
    unpushed_branches: Vec<String>,
}

impl Status {
    fn write<W: io::Write>(&self, mut out: W) -> io::Result<()> {
        writeln!(out, "{}", self.path.file_name().unwrap().to_str().unwrap())?;
        if self.dirty {
            writeln!(out, "\tDIRTY")?;
        }
        for branch in &self.untracked_branches {
            writeln!(out, "\tUNTRACKED {}", branch)?;
        }
        for branch in &self.unpushed_branches {
            writeln!(out, "\tUNPUSHED {}", branch)?;
        }
        Ok(())
    }
}

fn scan(path: PathBuf) -> anyhow::Result<Option<Status>> {
    let repo = match gix::open(&path) {
        Err(gix::open::Error::NotARepository { .. }) => return Ok(None),
        r => r,
    }?;

    // is_dirty fails if no HEAD
    let dirty = repo.head_id().is_err() || repo.is_dirty()?;

    let mut status = Status {
        path,
        dirty,
        ..Default::default()
    };

    for branch in repo.references()?.local_branches()? {
        // TODO: why doesn't `?` work here?
        let branch = branch.expect("valid reference");
        let name = branch.name().shorten();
        if let Some(upstream) = branch.remote_tracking_ref_name(gix::remote::Direction::Push) {
            let upstream = upstream?;
            let upstream: &gix::refs::FullNameRef = upstream.borrow();
            let upstream = repo.find_reference(upstream)?;

            // TODO: distinguish unpushed from unpulled
            if branch.id() != upstream.id() {
                status.unpushed_branches.push(name.to_string());
            }
        } else {
            status.untracked_branches.push(name.to_string());
        }
    }

    Ok(Some(status))
}

fn main() -> anyhow::Result<()> {
    let root = current_dir()?;

    for entry in read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Some(status) = scan(entry.path())? {
                status.write(stdout())?;
            }
        }
    }

    Ok(())
}

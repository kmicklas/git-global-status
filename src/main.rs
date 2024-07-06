use std::{
    env::current_dir,
    fs::read_dir,
    io::{self, stdout},
    path::PathBuf,
};

struct Status {
    path: PathBuf,
    dirty: bool,
}

impl Status {
    fn write<W: io::Write>(&self, mut out: W) -> io::Result<()> {
        writeln!(out, "{}", self.path.file_name().unwrap().to_str().unwrap())?;
        if self.dirty {
            writeln!(out, "\t{}", "DIRTY")?;
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
    let dirty = !repo.head_id().is_ok() || repo.is_dirty()?;

    Ok(Some(Status { path, dirty }))
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

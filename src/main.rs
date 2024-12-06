use std::{
    borrow::Borrow,
    collections::{BTreeMap, BTreeSet},
    fs::read_dir,
    path::{Path, PathBuf},
};

use clap::Parser;

#[derive(clap::Parser)]
struct Args {
    #[arg(short = 'C', default_value = ".")]
    root: PathBuf,
}

#[derive(Default)]
struct Status {
    dirty: bool,
    untracked_branches: Vec<String>,
    unpushed_branches: Vec<String>,
}

impl Status {
    fn is_clean(&self) -> bool {
        !self.dirty && self.untracked_branches.is_empty() && self.unpushed_branches.is_empty()
    }
}

fn scan(path: &Path) -> anyhow::Result<Option<Status>> {
    let repo = match gix::open(path) {
        Err(gix::open::Error::NotARepository { .. }) => return Ok(None),
        r => r,
    }?;

    let mut status = Status {
        // is_dirty fails if no HEAD
        dirty: repo.head_id().is_err() || repo.is_dirty()?,
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
    let args: Args = Args::parse();

    let (send, recv) = std::sync::mpsc::channel();

    std::thread::scope(move |s| {
        for entry in read_dir(args.root)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let send = send.clone();
                let path = entry.path();
                s.spawn(move || match scan(path.as_path()) {
                    Ok(None) => {}
                    Ok(Some(status)) => send.send(Ok((path, status))).unwrap(),
                    Err(e) => send.send(Err(e)).unwrap(),
                });
            }
        }
        Ok::<(), anyhow::Error>(())
    })?;

    let mut clean = BTreeSet::new();
    let mut dirty = BTreeMap::new();

    for status in recv {
        let (path, status) = status?;

        if status.is_clean() {
            clean.insert(path);
        } else {
            dirty.insert(path, status);
        }
    }

    for path in clean {
        println!("{}", path.file_name().unwrap().to_str().unwrap());
    }
    for (path, status) in dirty {
        println!("{}", path.file_name().unwrap().to_str().unwrap());

        if status.dirty {
            println!("\tDIRTY");
        }
        for branch in status.untracked_branches {
            println!("\tUNTRACKED {}", branch);
        }
        for branch in status.unpushed_branches {
            println!("\tUNPUSHED {}", branch);
        }
    }

    Ok(())
}

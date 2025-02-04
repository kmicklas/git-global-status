use std::{
    borrow::Borrow,
    collections::{BTreeMap, BTreeSet},
    fs::read_dir,
    path::{Path, PathBuf},
};

use anyhow::Context as _;
use clap::Parser;
use colored::Colorize;

#[derive(Parser)]
struct Args {
    #[arg(short = 'C', default_value = ".")]
    root: PathBuf,
}

#[derive(Default, PartialEq)]
struct Status {
    dirty: Vec<gix::status::index_worktree::Item>,
    untracked_branches: Vec<String>,
    unpushed_branches: Vec<String>,
}

impl Status {
    fn is_clean(&self) -> bool {
        self.eq(&Default::default())
    }
}

fn scan(path: &Path) -> anyhow::Result<Option<Status>> {
    let repo = match gix::open(path) {
        Err(gix::open::Error::NotARepository { .. }) => return Ok(None),
        r => r,
    }?;

    let mut dirty = repo
        .status(gix::progress::Discard)?
        .into_index_worktree_iter([])?
        .collect::<Result<Vec<_>, _>>()?;
    dirty.sort_by(|a, b| {
        (a.rela_path(), a.summary()).cmp(&(b.rela_path(), b.summary()))
    });

    let mut status = Status {
        dirty,
        ..Default::default()
    };

    for branch in repo.references()?.local_branches()? {
        // TODO: why doesn't `?` work here?
        let branch = branch.expect("valid reference");
        let name = branch.name().shorten();

        let mut pushed: Option<bool> = None;

        for direction in
            [gix::remote::Direction::Fetch, gix::remote::Direction::Push]
        {
            let Some(upstream) = branch.remote_tracking_ref_name(direction)
            else {
                continue;
            };
            let upstream = upstream?;
            let upstream: &gix::refs::FullNameRef = upstream.borrow();

            pushed = Some(false);

            match repo.find_reference(upstream) {
                Ok(upstream) => {
                    if branch.id() == upstream.id() {
                        pushed = Some(true);
                        break;
                    }
                }
                Err(gix::reference::find::existing::Error::NotFound {
                    ..
                }) => {}
                Err(e) => {
                    return Err(e).with_context(|| {
                        format!(
                            "failed to find upstream of branch {} in repo {}",
                            name,
                            path.display(),
                        )
                    })
                }
            }
        }

        match pushed {
            None => status.untracked_branches.push(name.to_string()),
            Some(false) => status.unpushed_branches.push(name.to_string()),
            Some(true) => {}
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
        println!("{}", path.file_name().unwrap().to_str().unwrap().green());
    }
    for (path, status) in dirty {
        println!("{}", path.file_name().unwrap().to_str().unwrap().red());

        for item in status.dirty {
            use gix::status::index_worktree::iter::Summary;
            println!(
                "\t{} {}",
                match item.summary() {
                    Some(s) => match s {
                        Summary::Removed => "D".red(),
                        Summary::Added => "A".green(),
                        Summary::Modified => "M".yellow(),
                        Summary::TypeChange => "T".yellow(),
                        Summary::Renamed => "R".blue(),
                        Summary::Copied => "C".magenta(),
                        Summary::IntentToAdd => "A".bright_green(),
                        Summary::Conflict => "X".red(),
                    },
                    None => "?".into(),
                },
                item.rela_path(),
            );
        }
        for branch in status.untracked_branches {
            println!("\t{} {}", "UNTRACKED".blue(), branch);
        }
        for branch in status.unpushed_branches {
            println!("\t{} {}", "UNPUSHED".magenta(), branch);
        }
    }

    Ok(())
}

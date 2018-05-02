#[macro_use]
extern crate error_chain;
extern crate regex;
extern crate structopt;
#[macro_use]
extern crate structopt_derive;
extern crate tabwriter;

use std::process::{Command, Stdio};
use std::path::PathBuf;
use std::io::Write;
use std::time::{Duration, SystemTime};
use regex::Regex;
use structopt::StructOpt;
use tabwriter::TabWriter;

error_chain! {
    foreign_links {
        Io(std::io::Error);
        Regex(regex::Error);
        Utf8(std::string::FromUtf8Error);
        SystemTimeError(std::time::SystemTimeError);
    }
}

/// Check or update Casks more reliably than brew cask upgrade
#[derive(StructOpt, Debug)]
#[structopt(name = "cask-update")]
struct Cli {
    /// List only
    #[structopt(short = "l", long = "list")]
    list: bool,
    /// Verbose mode
    #[structopt(short = "V", long = "verbose")]
    verbose: bool,
}

struct Cask<'a> {
    name: &'a str,
    installed: String,
    latest: String,
    updatable: bool,
}

quick_main!(run);

fn run() -> Result<()> {
    let cli = Cli::from_args();
    let output = Command::new("brew").arg("cask").arg("list").output()?;

    if !output.status.success() {
        bail!("Cannot execute brew cask list");
    }

    let casks = String::from_utf8(output.stdout)?;

    let latest_version_pattern = Regex::new(r".*: (.*)")?;
    let installed_path_pattern = Regex::new(r"(/usr/local/Caskroom/.*/(.*)) \(.*\)")?;

    let mut installed_casks: Vec<Cask> = casks
        .lines()
        .map(|s| s.trim())
        .map(|name| {
            let status = Command::new("brew")
                .arg("cask")
                .arg("info")
                .arg(name)
                .output()?;
            let info = String::from_utf8(status.stdout)?;
            let mut latest = None;
            let mut installed = None;
            let mut installed_path: Option<PathBuf> = None;

            let header: Vec<_> = info.lines().take(3).collect();

            if let Some(version) = latest_version_pattern.captures(header[0]) {
                latest = Some(version[1].to_string());
            }
            if let Some(path) = installed_path_pattern.captures(header[2]) {
                installed_path = Some(PathBuf::from(path[1].to_owned()));
                installed = Some(path[2].to_string());
            }

            let latest = latest.ok_or(format!("Unknown latest version for {}", name))?;
            let installed = installed.ok_or(format!("Unknown installed version for {}", name))?;
            let path = installed_path.ok_or(format!("Unknown installed path for {}", name))?;
            // TODO make list of always updatable casks configurable
            let updatable = if latest == "latest" {
                let modified = path.metadata()?.modified()?;
                let installed_duration = SystemTime::now().duration_since(modified)?;
                installed_duration > Duration::from_secs(20 * 3600)
            } else {
                latest != installed
            };

            Ok(Cask {
                name,
                installed,
                latest,
                updatable,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    installed_casks.sort_by(|c1, c2| {
        // actually put those that need to be updated at the bottom
        // as the list of casks is usually long and we want to know
        // about the updatable casks without scrolling
        c1.updatable.cmp(&c2.updatable).then(c1.name.cmp(&c2.name))
    });

    if cli.list {
        let mut tw = TabWriter::new(std::io::stdout());
        write!(&mut tw, "Cask\tInstalled\tLatest\tNeeds update\n")?;
        for cask in &installed_casks {
            write!(
                &mut tw,
                "{}\t{}\t{}\t{}\n",
                cask.name,
                cask.installed.chars().take(20).collect::<String>(),
                cask.latest.chars().take(20).collect::<String>(),
                if cask.updatable { "Yes" } else { "No" }
            )?;
        }

        tw.flush()?;
    } else {
        let updatable_casks = installed_casks.into_iter().filter(|c| c.updatable);

        for cask in updatable_casks {
            if cli.verbose {
                println!(
                    "Updating {} from {} to {}",
                    cask.name, cask.installed, cask.latest
                );
            }
            Command::new("brew")
                .arg("cask")
                .arg("reinstall")
                .arg(cask.name)
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()?;
        }
    }

    Ok(())
}

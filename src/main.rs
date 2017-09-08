#[macro_use]
extern crate error_chain;
extern crate structopt;
#[macro_use]
extern crate structopt_derive;
extern crate regex;
extern crate tabwriter;

use std::process::{Command, Stdio};
use std::io::Write;
use regex::Regex;
use structopt::StructOpt;
use tabwriter::TabWriter;

error_chain! {
    foreign_links {
        Io(std::io::Error);
        Regex(regex::Error);
        Utf8(std::string::FromUtf8Error);
    }
}

/// Check or update Casks
#[derive(StructOpt, Debug)]
#[structopt(name = "cask-update")]
struct Cli {
    /// Long format display
    #[structopt(short = "l", long = "long")]
    long: bool,
    /// Perform update
    #[structopt(short = "u", long = "update")]
    update: bool,
    /// Verbose mode
    #[structopt(short = "V", long = "verbose")]
    verbose: bool,
}

struct Cask<'a> {
    name: &'a str,
    installed: String,
    current: String,
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

    let current_version_pattern = Regex::new(r".*: (.*)")?;
    let installed_version_pattern = Regex::new(r"/usr/local/Caskroom/.*/(.*) \(.*\)")?;

    let installed_casks: Vec<Cask> = casks
        .lines()
        .map(|s| s.trim())
        .map(|name| {
            let status = Command::new("brew")
                .arg("cask")
                .arg("info")
                .arg(name)
                .output()?;
            let info = String::from_utf8(status.stdout)?;
            let mut current = None;
            let mut installed = None;

            let header: Vec<_> = info.lines().take(3).collect();

            if let Some(version) = current_version_pattern.captures(header[0]) {
                current = Some(version[1].to_string());
            }
            if let Some(version) = installed_version_pattern.captures(header[2]) {
                installed = Some(version[1].to_string());
            }

            let current = current.ok_or(
                format!("Unknown current version for {}", name),
            )?;
            let installed = installed.ok_or(
                format!("Unknown installed version for {}", name),
            )?;
            let updatable = current != installed;

            Ok(Cask {
                name,
                installed,
                current,
                updatable,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    if cli.verbose {
        let mut tw = TabWriter::new(std::io::stdout());
        write!(&mut tw, "Cask\tInstalled\tCurrent\tStatus\n")?;
        for cask in &installed_casks {
            write!(
                &mut tw,
                "{}\t{}\t{}\t{}\n",
                cask.name,
                cask.installed,
                cask.current,
                if cask.updatable {
                    "outdated"
                } else {
                    "up to date"
                }
            )?;
        }

        tw.flush()?;
    }

    let updatable_casks = installed_casks.into_iter().filter(|c| c.updatable);

    for cask in updatable_casks {
        if cli.update {
            if cli.long {
                println!(
                    "Updating {} from {} to {}",
                    cask.name,
                    cask.installed,
                    cask.current
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
        } else if cli.long {
            println!(
                "{}: installed {}, current: {}",
                cask.name,
                cask.installed,
                cask.current
            );
        } else if !cli.verbose {
            println!("{}", cask.name);
        }
    }

    Ok(())
}

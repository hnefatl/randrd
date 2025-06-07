use std::path::PathBuf;

use anyhow::{self, Context};
use clap::Parser;
use pidfile;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value = "/var/run/randrd.pid")]
    pid_file: PathBuf,
}

fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();
    let _pid_file = pidfile::PidFile::new(args.pid_file).context("Opening PID lockfile")?;
    Ok(())
}

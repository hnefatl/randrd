use std::path::PathBuf;

use anyhow::{self, Context};
use clap::Parser;
use pidfile;
use xrandr;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value = "/var/run/randrd.pid")]
    pid_file: PathBuf,
}

fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();
    let _pid_file = pidfile::PidFile::new(args.pid_file).context("Opening PID lockfile")?;
    let mut xhandle = xrandr::XHandle::open()?;
    for monitor in xhandle.monitors()? {
        println!("Monitor: {}", monitor.name);
        for output in monitor.outputs {
            println!("Output: {}", output.name);
            match output.edid().as_deref().map(edid::parse) {
                Some(nom::IResult::Done(_, edid_value)) => {
                    println!("{:?}", edid_value)
                }
                e => println!("Failed to parse EDID: {:?}", e),
            }
            println!();
        }
    }
    Ok(())
}

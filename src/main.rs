use std::{collections::HashMap, path::PathBuf};

use anyhow::{self, Context};
use clap::Parser;
use pidfile;
use serde::{Deserialize, Serialize};
use xrandr;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value = "/var/run/randrd.pid")]
    pid_file: PathBuf,

    #[arg(long, value_parser=|s: &str| -> anyhow::Result<Config> {Ok(ron::from_str(s)?)})]
    config: Config,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Config {
    // Monitor name to desired config
    // Keying on name might need adjusting in the future since there's no guarantee that monitor names are unique between my computers.
    monitors: HashMap<String, MonitorConfig>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
struct MonitorConfig {
    width: u64,
    height: u64,
    // Refresh rate may not match exactly, closest wins.
    refresh_rate: f32,
}

struct MonitorInfo {
    inner: xrandr::Monitor,
}
impl MonitorInfo {
    fn is_enabled(&self) -> bool {
        return self
            .inner
            .outputs
            .iter()
            .any(|o| o.connected && o.current_mode.is_some());
    }
}

fn main() -> anyhow::Result<()> {
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
            println!("Modes:");
            for mode in xrandr::ScreenResources::new(&mut xhandle)?.modes() {
                println!("  {}: {} {} {}", mode.name, mode.width, mode.height, mode.rate);
            }
        }
    }
    Ok(())
}

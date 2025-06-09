use core::f64;
use std::{collections::HashMap, path::PathBuf};

use anyhow::{self, Context, bail};
use clap::Parser;
use serde::{Deserialize, Serialize};

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
    monitors: HashMap<String, MonitorSpec>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
struct MonitorSpec {
    width: u32,
    height: u32,
    // Refresh rate may not match exactly, closest wins.
    refresh_rate: Option<f64>,
}
impl MonitorSpec {
    const REFRESH_RATE_TOLERANCE: f64 = 1.0;

    fn get_compatible_modes<'a>(&self, modes: &'a [xrandr::Mode]) -> Vec<&'a xrandr::Mode> {
        modes
            .iter()
            .filter(|m| (self.width, self.height) == (m.width, m.height))
            .filter(|m| {
                self.refresh_rate
                    .is_none_or(|r| f64::abs(r - m.rate) < Self::REFRESH_RATE_TOLERANCE)
            })
            .collect()
    }
}

fn get_edid(output: &xrandr::Output) -> anyhow::Result<edid::EDID> {
    match output.edid().as_deref().map(edid::parse) {
        Some(nom::IResult::Done(_, edid_value)) => Ok(edid_value),
        e => anyhow::bail!("Failed to parse EDID: {:?}", e),
    }
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let _pid_file = pidfile::PidFile::new(args.pid_file).context("Opening PID lockfile")?;

    let mut xhandle = xrandr::XHandle::open()?;
    let modes = xrandr::ScreenResources::new(&mut xhandle)?.modes();
    for monitor in xhandle.monitors()? {
        let Some(monitor_config) = args.config.monitors.get(&monitor.name) else {
            println!("Skipping unconfigured monitor: {}", monitor.name);
            continue;
        };
        let [output] = &monitor.outputs[..] else {
            println!("Skipping monitor with >1 output: {}", monitor.name);
            continue;
        };

        let compatible_modes = monitor_config.get_compatible_modes(&modes);
        if compatible_modes.is_empty() {
            println!(
                "Unable to find compatible modes for monitor {}: ",
                monitor.name
            );
            continue;
        };
        if output
            .current_mode
            .is_some_and(|id| compatible_modes.iter().any(|m| m.xid == id))
        {
            println!(
                "Skipping monitor already assigned a compatible mode: {}",
                monitor.name
            );
            continue;
        }
        if let [compatible_mode] = compatible_modes[..] {
            println!("Single compatible mode, updating: {:?}", compatible_mode);
            xhandle.set_mode(output, compatible_mode)?;
        } else {
            println!("Expected exactly one compatible mode for monitor {}, found: {:?}", monitor.name, compatible_modes);
        }
    }
    Ok(())
}

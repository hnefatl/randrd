use core::f64;
use std::{collections::HashMap, ops::Deref, path::PathBuf};

use anyhow::{self, Context};
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
// Desired config for a monitor.
#[derive(Debug, Clone, Deserialize, Serialize)]
struct MonitorSpec {
    width: u32,
    height: u32,
    refresh_rate: Option<f64>,  // Refresh rate may not match exactly, closest wins.

    #[serde(default)]
    primary: bool,
    #[serde(default)]
    rotation: xrandr::Rotation,
    x: u32,
    y: u32,
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

enum MonitorResult {
    Ok(),
    Skipped(String),
}

fn process_monitor(
    xhandle: &mut xrandr::XHandle,
    config: &Config,
    monitor: &xrandr::Monitor,
    modes: &Vec<xrandr::Mode>,
) -> anyhow::Result<MonitorResult> {
    macro_rules! skip {
        ($($arg:tt)*) => { return Ok(MonitorResult::Skipped(format!($($arg)*))); };
    }

    let Some(monitor_config) = config.monitors.get(&monitor.name) else {
        skip!("unconfigured");
    };
    let [output] = &monitor.outputs[..] else {
        skip!(
            "has >1 output: {:?}",
            monitor.outputs.iter().map(|o| &o.name)
        );
    };

    let compatible_modes = monitor_config.get_compatible_modes(&modes);
    if compatible_modes.is_empty() {
        skip!("unable to find compatible modes");
    };
    if let Some(current_mode) = output
        .current_mode
        .and_then(|id| compatible_modes.iter().find(|m| m.xid == id))
    {
        skip!("already assigned a compatible mode: {}", current_mode.name);
    }
    if let [compatible_mode] = compatible_modes[..] {
        xhandle.set_mode(output, compatible_mode)?;
        xhandle.set_rotation(output, monitor_config.rotation.into())?;
        xhandle.set_position(output, relation, relative_output)
        return Ok(MonitorResult::Ok());
    }
    skip!(
        "found >1 compatible modes, can't choose: {:?}",
        compatible_modes
    );
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let _pid_file = pidfile::PidFile::new(args.pid_file).context("Opening PID lockfile")?;

    let mut xhandle = xrandr::XHandle::open()?;
    let modes = xrandr::ScreenResources::new(&mut xhandle)?.modes();

    for monitor in xhandle.monitors()? {
        match process_monitor(&mut xhandle, &args.config, &monitor, &modes) {
            Ok(MonitorResult::Ok()) => {}
            Ok(MonitorResult::Skipped(reason)) => {
                println!("Skipped monitor {}: {}", monitor.name, reason)
            }
            Err(e) => println!("{}", e),
        }
    }
    Ok(())
}

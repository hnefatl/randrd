use core::f64;
use std::{collections::HashMap, intrinsics::copysignf16, path::PathBuf};

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
    fn get_compatible_modes<'a>(&self, modes: &'a [xrandr::Mode]) -> Vec<&'a xrandr::Mode> {
        let mut compatible: &mut dyn Iterator<Item = &'a xrandr::Mode> = &modes.iter();
        compatible = compatible.filter(|m| (self.width, self.height) == (m.width, m.height));

        compatible.collect()
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
        let Some(closest_mode) = get_closest_mode(monitor_config, &modes) else {
            println!("Unable to find closest mode for monitor {}: ", monitor.name);
            continue;
        };
        let [output] = &monitor.outputs[..] else {
            println!("Skipping monitor with >1 output: {}", monitor.name);
            continue;
        };
        if output.current_mode == Some(closest_mode.xid) {
            println!(
                "Skipping monitor already assigned closest mode: {}",
                monitor.name
            );
            continue;
        }
        let current_mode_desc = output
            .current_mode
            .and_then(|id| modes.iter().find(|m| m.xid == id));
        println!("{:?} {:?}", output.current_mode, current_mode_desc);
        println!("{} {:?}", closest_mode.xid, closest_mode);
        println!("Need to update");
    }
    Ok(())
}

use core::f64;
use std::{collections::HashMap, path::PathBuf};

use anyhow::{self, Context, bail};
use clap::Parser;
use serde::{Deserialize, Serialize};
use xrandr::ScreenResources;

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
    refresh_rate: Option<f64>, // Refresh rate may not match exactly, closest wins.

    #[serde(default)]
    primary: bool,
    #[serde(default)]
    rotation: xrandr::Rotation,
    x: i32,
    y: i32,
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

// An `xrandr::Mode` object with limited equality scope.
#[derive(Debug)]
struct LimitedMode {
    inner: xrandr::Mode,
}
impl PartialEq for LimitedMode {
    fn eq(&self, other: &Self) -> bool {
        return self.inner.height == other.inner.height;
    }
}
impl Eq for LimitedMode {}

#[derive(Default, Debug, PartialEq, Eq)]
struct Diff {
    primary: Option<bool>,
    position: Option<(i32, i32)>,
    rotation: Option<xrandr::Rotation>,
    mode: Option<LimitedMode>,
}

fn get_monitor_diff<'a>(
    xhandle: &mut xrandr::XHandle,
    config: &Config,
    monitor: &'a xrandr::Monitor,
    screen_resources: &xrandr::ScreenResources,
) -> anyhow::Result<(&'a xrandr::Output, Diff)> {
    let mut diff = Diff::default();

    let Some(monitor_config) = config.monitors.get(&monitor.name) else {
        bail!("unconfigured");
    };
    let [output] = &monitor.outputs[..] else {
        bail!(
            "has >1 output: {:?}",
            monitor.outputs.iter().map(|o| &o.name)
        );
    };
    let Some(crtc_id) = output.crtc else {
        bail!(
            "required exactly 1 CRTC associated with output {}, got {:?} and {:?}",
            output.name,
            output.crtc,
            output.crtcs
        );
    };
    let crtc = screen_resources.crtc(xhandle, crtc_id)?;
    if crtc.rotation != monitor_config.rotation {
        diff.rotation = Some(monitor_config.rotation);
    }
    if (crtc.x, crtc.y) != (monitor_config.x, monitor_config.y) {
        diff.position = Some((monitor_config.x, monitor_config.y));
    }

    let compatible_modes = monitor_config.get_compatible_modes(&screen_resources.modes);
    if compatible_modes.is_empty() {
        bail!(
            "unable to find compatible modes: {:?}",
            screen_resources.modes
        );
    };
    if !output
        .current_mode
        .is_some_and(|id| compatible_modes.iter().any(|m| m.xid == id))
    {
        let [compatible_mode] = compatible_modes[..] else {
            bail!(
                "found >1 compatible modes, can't choose: {:?}",
                compatible_modes
            );
        };
        diff.mode = Some(LimitedMode {
            inner: compatible_mode.clone(),
        });
    }
    Ok((output, diff))
}

fn apply_monitor_diff(
    xhandle: &mut xrandr::XHandle,
    output: &xrandr::Output,
    diff: Diff,
) -> anyhow::Result<()> {
    match diff.primary {
        // TODO: why doesn't this have an error return type?
        Some(true) => xhandle.set_primary(output),
        // TODO: no library call available here???
        Some(false) => unimplemented!(),
        _ => {}
    }
    if let Some((x, y)) = diff.position {
        xhandle.set_absolute_position(output, x, y)?;
    }
    if let Some(rotation) = diff.rotation {
        xhandle.set_rotation(output, &rotation)?;
    }
    if let Some(mode) = diff.mode {
        xhandle.set_mode(output, &mode.inner)?;
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let _pid_file = pidfile::PidFile::new(args.pid_file).context("Opening PID lockfile")?;

    let mut xhandle = xrandr::XHandle::open()?;
    let sr = xrandr::ScreenResources::new(&mut xhandle)?;

    for monitor in xhandle.monitors()? {
        match get_monitor_diff(&mut xhandle, &args.config, &monitor, &sr) {
            Ok((_, diff)) if diff == Diff::default() => {}
            Ok((output, diff)) => {
                println!("Monitor {} has diff: {:?}", monitor.name, diff);
                apply_monitor_diff(&mut xhandle, output, diff)?;
            }
            Err(e) => println!("{:?}", e),
        }
    }
    Ok(())
}

use anyhow::Result;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize, Default)]
pub struct Config {
    pub device: Option<DeviceConfig>,
    pub scroll: Option<ScrollConfig>,
    pub pointer: Option<PointerConfig>,
    pub interrupt: Option<InterruptConfig>,
    pub log_level: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct DeviceConfig {
    pub path: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ScrollConfig {
    pub enabled: Option<bool>,
    pub damping: Option<f64>,
    pub damping_curve: Option<String>,
    pub phase_threshold: Option<f64>,
    pub linear_decel_ms: Option<i32>,
    pub linear_stop_hires: Option<i32>,
    pub time_constant_ms: Option<f64>,
    pub stop_threshold: Option<f64>,
    pub tail_scroll_ms: Option<u64>,
    pub min_velocity: Option<f64>,
    pub scroll_factor: Option<f64>,
    pub tp_to_hires: Option<f64>,
    pub velocity_stale_ms: Option<u64>,
    pub natural_scroll: Option<bool>,
    pub multitouch_cooldown: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
pub struct PointerConfig {
    pub enabled: Option<bool>,
    pub drag: Option<f64>,
    pub speed_factor: Option<f64>,
    pub min_velocity: Option<f64>,
}

#[derive(Debug, Deserialize, Default)]
pub struct InterruptConfig {
    pub enabled: Option<bool>,
}

pub const DEFAULT_MODE: &str = "scroll";
pub const DEFAULT_DAMPING: f64 = 0.05;
pub const DEFAULT_DAMPING_CURVE: &str = "dual";
pub const DEFAULT_PHASE_THRESHOLD: f64 = 25.0;
pub const DEFAULT_LINEAR_DECEL_MS: i32 = 384;
pub const DEFAULT_LINEAR_STOP_HIRES: i32 = 1;
pub const DEFAULT_TIME_CONSTANT_MS: f64 = 325.0;
pub const DEFAULT_STOP_THRESHOLD: f64 = 60.0;
pub const DEFAULT_TAIL_SCROLL_MS: u64 = 0;
pub const DEFAULT_MIN_SCROLL_VELOCITY: f64 = 120.0;
pub const DEFAULT_SCROLL_FACTOR: f64 = 1.0;
pub const DEFAULT_TP_TO_HIRES: f64 = 5.0;
pub const DEFAULT_VELOCITY_STALE_MS: u64 = 150;
pub const DEFAULT_NATURAL_SCROLL: bool = false;
pub const DEFAULT_POINTER_DRAG: f64 = 0.15;
pub const DEFAULT_POINTER_SPEED_FACTOR: f64 = 0.0075;
pub const DEFAULT_POINTER_MIN_VELOCITY: f64 = 2000.0;
pub const DEFAULT_MULTITOUCH_COOLDOWN: u64 = 500;
pub const DEFAULT_LOG_LEVEL: &str = "info";

pub fn load(path: &Path) -> Result<Config> {
    let content = std::fs::read_to_string(path)?;
    let config: Config = toml::from_str(&content)?;
    Ok(config)
}

/// Resolve final Args: CLI (if set) > config file > hardcoded defaults.
pub fn resolve(cli: &crate::Args, cfg: &Config) -> crate::ResolvedArgs {
    let dev = cfg.device.as_ref();
    let scroll = cfg.scroll.as_ref();
    let pointer = cfg.pointer.as_ref();
    let interrupt = cfg.interrupt.as_ref();

    let mode = cli.mode.clone().unwrap_or_else(|| {
        let scroll_on = scroll.and_then(|s| s.enabled).unwrap_or(true);
        let pointer_on = pointer.and_then(|p| p.enabled).unwrap_or(false);
        match (scroll_on, pointer_on) {
            (true, true) => "both".into(),
            (false, true) => "pointer".into(),
            _ => DEFAULT_MODE.into(),
        }
    });

    crate::ResolvedArgs {
        device: cli
            .device
            .clone()
            .or_else(|| dev.and_then(|d| d.path.clone())),
        device_name: cli
            .device_name
            .clone()
            .or_else(|| dev.and_then(|d| d.name.clone())),
        mode,
        damping: cli
            .damping
            .unwrap_or_else(|| scroll.and_then(|s| s.damping).unwrap_or(DEFAULT_DAMPING)),
        damping_curve: resolve_damping_curve(cli, scroll),
        phase_threshold: cli.phase_threshold.unwrap_or_else(|| {
            scroll
                .and_then(|s| s.phase_threshold)
                .unwrap_or(DEFAULT_PHASE_THRESHOLD)
        }),
        linear_decel_ms: cli.linear_decel_ms.unwrap_or_else(|| {
            scroll
                .and_then(|s| s.linear_decel_ms)
                .unwrap_or(DEFAULT_LINEAR_DECEL_MS)
        }),
        linear_stop_hires: cli.linear_stop_hires.unwrap_or_else(|| {
            scroll
                .and_then(|s| s.linear_stop_hires)
                .unwrap_or(DEFAULT_LINEAR_STOP_HIRES)
        }),
        time_constant_ms: cli.time_constant_ms.unwrap_or_else(|| {
            scroll
                .and_then(|s| s.time_constant_ms)
                .unwrap_or(DEFAULT_TIME_CONSTANT_MS)
        }),
        stop_threshold: cli.stop_threshold.unwrap_or_else(|| {
            scroll
                .and_then(|s| s.stop_threshold)
                .unwrap_or(DEFAULT_STOP_THRESHOLD)
        }),
        tail_scroll_ms: cli.tail_scroll_ms.unwrap_or_else(|| {
            scroll
                .and_then(|s| s.tail_scroll_ms)
                .unwrap_or(DEFAULT_TAIL_SCROLL_MS)
        }),
        min_scroll_velocity: cli.min_scroll_velocity.unwrap_or_else(|| {
            scroll
                .and_then(|s| s.min_velocity)
                .unwrap_or(DEFAULT_MIN_SCROLL_VELOCITY)
        }),
        scroll_factor: cli.scroll_factor.unwrap_or_else(|| {
            scroll
                .and_then(|s| s.scroll_factor)
                .unwrap_or(DEFAULT_SCROLL_FACTOR)
        }),
        tp_to_hires: cli.tp_to_hires.unwrap_or_else(|| {
            scroll
                .and_then(|s| s.tp_to_hires)
                .unwrap_or(DEFAULT_TP_TO_HIRES)
        }),
        velocity_stale_ms: cli.velocity_stale_ms.unwrap_or_else(|| {
            scroll
                .and_then(|s| s.velocity_stale_ms)
                .unwrap_or(DEFAULT_VELOCITY_STALE_MS)
        }),
        pointer_drag: cli
            .pointer_drag
            .unwrap_or_else(|| pointer.and_then(|p| p.drag).unwrap_or(DEFAULT_POINTER_DRAG)),
        pointer_speed_factor: cli.pointer_speed_factor.unwrap_or_else(|| {
            pointer
                .and_then(|p| p.speed_factor)
                .unwrap_or(DEFAULT_POINTER_SPEED_FACTOR)
        }),
        pointer_min_velocity: cli.pointer_min_velocity.unwrap_or_else(|| {
            pointer
                .and_then(|p| p.min_velocity)
                .unwrap_or(DEFAULT_POINTER_MIN_VELOCITY)
        }),
        multitouch_cooldown: cli.multitouch_cooldown.unwrap_or_else(|| {
            scroll
                .and_then(|s| s.multitouch_cooldown)
                .unwrap_or(DEFAULT_MULTITOUCH_COOLDOWN)
        }),
        natural_scroll: if cli.natural_scroll {
            true
        } else if cli.no_natural_scroll {
            false
        } else {
            scroll
                .and_then(|s| s.natural_scroll)
                .unwrap_or(DEFAULT_NATURAL_SCROLL)
        },
        no_interrupt: cli.no_interrupt || interrupt.and_then(|i| i.enabled).is_some_and(|e| !e),
        dry: cli.dry,
        log_level: cli.log_level.clone().unwrap_or_else(|| {
            cfg.log_level
                .clone()
                .unwrap_or_else(|| DEFAULT_LOG_LEVEL.into())
        }),
    }
}

fn resolve_damping_curve(cli: &crate::Args, scroll: Option<&ScrollConfig>) -> String {
    cli.damping_curve
        .clone()
        .or_else(|| scroll.and_then(|s| s.damping_curve.clone()))
        .unwrap_or_else(|| DEFAULT_DAMPING_CURVE.into())
}

fn is_set<T>(cli_val: Option<T>, cfg_val: Option<T>) -> bool {
    cli_val.is_some() || cfg_val.is_some()
}

fn warn_unused(curve: &str, params: &[(&str, bool)]) {
    for (name, set) in params {
        if *set {
            log::warn!(
                "{} is set but has no effect with damping_curve=\"{}\"",
                name,
                curve
            );
        }
    }
}

pub fn warn_unused_curve_params(cli: &crate::Args, cfg: &Config, resolved: &crate::ResolvedArgs) {
    let s = cfg.scroll.as_ref();

    let damping = is_set(cli.damping, s.and_then(|c| c.damping));
    let phase_threshold = is_set(cli.phase_threshold, s.and_then(|c| c.phase_threshold));
    let linear_decel_ms = is_set(cli.linear_decel_ms, s.and_then(|c| c.linear_decel_ms));
    let linear_stop_hires = is_set(cli.linear_stop_hires, s.and_then(|c| c.linear_stop_hires));
    let time_constant_ms = is_set(cli.time_constant_ms, s.and_then(|c| c.time_constant_ms));
    let stop_threshold = is_set(cli.stop_threshold, s.and_then(|c| c.stop_threshold));

    match resolved.damping_curve.as_str() {
        "expo" => warn_unused(
            "expo",
            &[
                ("phase_threshold", phase_threshold),
                ("linear_decel_ms", linear_decel_ms),
                ("linear_stop_hires", linear_stop_hires),
                ("time_constant_ms", time_constant_ms),
                ("stop_threshold", stop_threshold),
            ],
        ),
        "dual" => warn_unused(
            "dual",
            &[
                ("time_constant_ms", time_constant_ms),
                ("stop_threshold", stop_threshold),
            ],
        ),
        "macos" => warn_unused(
            "macos",
            &[
                ("damping", damping),
                ("phase_threshold", phase_threshold),
                ("linear_decel_ms", linear_decel_ms),
                ("linear_stop_hires", linear_stop_hires),
            ],
        ),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn cli(args: &[&str]) -> crate::Args {
        crate::Args::parse_from(args)
    }

    fn scroll_cfg(scroll: ScrollConfig) -> Config {
        Config {
            scroll: Some(scroll),
            ..Default::default()
        }
    }

    #[test]
    fn cli_beats_config_beats_default() {
        let cfg = scroll_cfg(ScrollConfig {
            damping: Some(0.2),
            ..Default::default()
        });
        assert_eq!(resolve(&cli(&["rinertia"]), &cfg).damping, 0.2);
        assert_eq!(
            resolve(&cli(&["rinertia", "--damping", "0.3"]), &cfg).damping,
            0.3
        );
        assert_eq!(
            resolve(&cli(&["rinertia"]), &Config::default()).damping,
            DEFAULT_DAMPING
        );
    }

    #[test]
    fn natural_scroll_resolution() {
        let cfg = scroll_cfg(ScrollConfig {
            natural_scroll: Some(true),
            ..Default::default()
        });
        assert!(resolve(&cli(&["rinertia"]), &cfg).natural_scroll);
        assert!(!resolve(&cli(&["rinertia", "--no-natural-scroll"]), &cfg).natural_scroll);
        assert!(
            resolve(&cli(&["rinertia", "--natural-scroll"]), &Config::default()).natural_scroll
        );
        assert_eq!(
            resolve(&cli(&["rinertia"]), &Config::default()).natural_scroll,
            DEFAULT_NATURAL_SCROLL
        );
    }

    #[test]
    fn mode_from_cli_or_enabled_flags() {
        assert_eq!(
            resolve(&cli(&["rinertia"]), &Config::default()).mode,
            "scroll"
        );

        let both = Config {
            scroll: Some(ScrollConfig {
                enabled: Some(true),
                ..Default::default()
            }),
            pointer: Some(PointerConfig {
                enabled: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(resolve(&cli(&["rinertia"]), &both).mode, "both");

        let pointer_only = Config {
            scroll: Some(ScrollConfig {
                enabled: Some(false),
                ..Default::default()
            }),
            pointer: Some(PointerConfig {
                enabled: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(resolve(&cli(&["rinertia"]), &pointer_only).mode, "pointer");

        assert_eq!(
            resolve(&cli(&["rinertia", "--mode", "pointer"]), &Config::default()).mode,
            "pointer"
        );
    }

    #[test]
    fn toml_round_trip() {
        let cfg: Config = toml::from_str(
            r#"
            log_level = "debug"
            [scroll]
            damping = 0.07
            natural_scroll = true
            "#,
        )
        .unwrap();
        let r = resolve(&cli(&["rinertia"]), &cfg);
        assert_eq!(r.damping, 0.07);
        assert!(r.natural_scroll);
        assert_eq!(r.log_level, "debug");
    }
}

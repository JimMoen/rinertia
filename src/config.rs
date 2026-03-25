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
    pub decay_mode: Option<String>,
    pub phase_threshold: Option<f64>,
    pub linear_decel_ms: Option<i32>,
    pub linear_stop_hires: Option<i32>,
    pub min_velocity: Option<f64>,
    pub scroll_factor: Option<f64>,
    pub tp_to_hires: Option<f64>,
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
pub const DEFAULT_DECAY_MODE: &str = "dual";
pub const DEFAULT_PHASE_THRESHOLD: f64 = 360.0;
pub const DEFAULT_LINEAR_DECEL_MS: i32 = 384;
pub const DEFAULT_LINEAR_STOP_HIRES: i32 = 1;
pub const DEFAULT_MIN_SCROLL_VELOCITY: f64 = 120.0;
pub const DEFAULT_SCROLL_FACTOR: f64 = 1.0;
pub const DEFAULT_TP_TO_HIRES: f64 = 5.0;
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
        decay_mode: cli.decay_mode.clone().unwrap_or_else(|| {
            scroll
                .and_then(|s| s.decay_mode.clone())
                .unwrap_or_else(|| DEFAULT_DECAY_MODE.into())
        }),
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
        no_interrupt: cli.no_interrupt || interrupt.and_then(|i| i.enabled).map_or(false, |e| !e),
        dry: cli.dry,
        log_level: cli.log_level.clone().unwrap_or_else(|| {
            cfg.log_level
                .clone()
                .unwrap_or_else(|| DEFAULT_LOG_LEVEL.into())
        }),
    }
}

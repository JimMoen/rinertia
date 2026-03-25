use anyhow::Result;
use clap::Parser;
use log;
use std::sync::mpsc;
use std::thread;

mod config;
mod device_discovery;
mod interrupt;
mod momentum;
mod touchpad;
mod virtual_device;

/// Inertial scrolling and pointer movement daemon for Linux touchpads.
///
/// Passively monitors touchpad events (no device grab) and injects
/// momentum scroll/pointer events via a virtual uinput device.
#[derive(Parser, Debug, Clone)]
#[command(name = "rinertia", version, about)]
pub struct Args {
    /// Path to TOML config file
    #[arg(short, long)]
    pub config: Option<String>,

    /// Touchpad device path (auto-detect if omitted)
    #[arg(short, long)]
    pub device: Option<String>,

    /// Match touchpad by name substring (e.g. "ELAN", "Synaptics")
    #[arg(short = 'n', long)]
    pub device_name: Option<String>,

    /// Operating mode: scroll, pointer, both
    #[arg(long)]
    pub mode: Option<String>,

    /// Damping coefficient (0.0 ~ 1.0). Higher = more resistance = faster deceleration
    #[arg(long)]
    pub damping: Option<f64>,

    /// Damping curve: "dual" (expo + linear tail), "expo" (pure exponential), "macos" (macOS-style time constant)
    #[arg(long)]
    pub damping_curve: Option<String>,

    /// Velocity threshold to transition from exponential to linear phase (hires per 8ms)
    #[arg(long)]
    pub phase_threshold: Option<f64>,

    /// Duration of linear deceleration phase in ms
    #[arg(long)]
    pub linear_decel_ms: Option<i32>,

    /// Linear phase stops when output reaches this value (hires units)
    #[arg(long)]
    pub linear_stop_hires: Option<i32>,

    /// [macos curve] Time constant in ms controlling deceleration feel (default: 325)
    #[arg(long)]
    pub time_constant_ms: Option<f64>,

    /// [macos curve] Velocity threshold to stop momentum (hires/sec, default: 60.0)
    #[arg(long)]
    pub stop_threshold: Option<f64>,

    /// Tail scroll: emit minimum hires for this many ms after deceleration ends (0 = off)
    #[arg(long)]
    pub tail_scroll_ms: Option<u64>,

    /// Minimum velocity to trigger scroll momentum
    #[arg(long)]
    pub min_scroll_velocity: Option<f64>,

    /// Scroll speed multiplier
    #[arg(long)]
    pub scroll_factor: Option<f64>,

    /// Touchpad-to-hires conversion factor
    #[arg(long)]
    pub tp_to_hires: Option<f64>,

    /// Drag coefficient for pointer inertia (0.0 ~ 1.0)
    #[arg(long)]
    pub pointer_drag: Option<f64>,

    /// Scale factor from touchpad units to virtual mouse units
    #[arg(long)]
    pub pointer_speed_factor: Option<f64>,

    /// Minimum touchpad speed to trigger pointer inertia
    #[arg(long)]
    pub pointer_min_velocity: Option<f64>,

    /// Multitouch cooldown in ms
    #[arg(long)]
    pub multitouch_cooldown: Option<u64>,

    /// Disable keyboard/mouse interrupt detection
    #[arg(long)]
    pub no_interrupt: bool,

    /// Dry mode: don't create virtual device, only log
    #[arg(long)]
    pub dry: bool,

    /// Log level (off, error, warn, info, debug, trace)
    #[arg(long)]
    pub log_level: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedArgs {
    pub device: Option<String>,
    pub device_name: Option<String>,
    pub mode: String,
    pub damping: f64,
    pub damping_curve: String,
    pub phase_threshold: f64,
    pub linear_decel_ms: i32,
    pub linear_stop_hires: i32,
    pub time_constant_ms: f64,
    pub stop_threshold: f64,
    pub tail_scroll_ms: u64,
    pub min_scroll_velocity: f64,
    pub scroll_factor: f64,
    pub tp_to_hires: f64,
    pub pointer_drag: f64,
    pub pointer_speed_factor: f64,
    pub pointer_min_velocity: f64,
    pub multitouch_cooldown: u64,
    pub no_interrupt: bool,
    pub dry: bool,
    pub log_level: String,
}

#[derive(Debug, Clone)]
pub enum MomentumMessage {
    StartScroll {
        velocity_hires_per_sec: f64,
        axis: ScrollAxis,
    },
    StartPointer {
        vx: f64,
        vy: f64,
    },
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollAxis {
    Vertical,
    Horizontal,
}

fn main() -> Result<()> {
    let cli = Args::parse();

    let cfg = match &cli.config {
        Some(path) => {
            let p = std::path::Path::new(path);
            config::load(p)?
        }
        None => config::Config::default(),
    };

    let args = config::resolve(&cli, &cfg);

    env_logger::Builder::new()
        .filter_module(
            "rinertia",
            args.log_level.parse().unwrap_or(log::LevelFilter::Info),
        )
        .parse_default_env()
        .init();

    if cli.config.is_some() {
        log::info!("Loaded config: {}", cli.config.as_deref().unwrap());
    }
    config::warn_unused_curve_params(&cli, &args);

    let touchpad_path =
        device_discovery::find_touchpad(args.device.as_deref(), args.device_name.as_deref())?;
    log::info!("Using touchpad: {}", touchpad_path.display());

    let vdev = if args.dry {
        log::info!("Dry mode: no virtual device created");
        None
    } else {
        let enable_scroll = args.mode == "scroll" || args.mode == "both";
        let enable_pointer = args.mode == "pointer" || args.mode == "both";
        Some(virtual_device::VirtualDevice::new(
            enable_scroll,
            enable_pointer,
        )?)
    };

    let (tx, rx) = mpsc::channel::<MomentumMessage>();
    let tx_interrupt = tx.clone();

    let tp_device = evdev::Device::open(&touchpad_path)?;
    let tp_phys = device_discovery::get_phys(&tp_device);
    log::info!(
        "Touchpad: {} [phys: {}]",
        tp_device.name().unwrap_or("unknown"),
        tp_phys.as_deref().unwrap_or("?")
    );

    let args_clone = args.clone();
    let touchpad_thread = thread::Builder::new()
        .name("listener".into())
        .spawn(move || {
            touchpad::run_listener(tp_device, tx, &args_clone);
        })?;

    let interrupt_thread = if !args.no_interrupt {
        let tp_phys_clone = tp_phys.clone();
        Some(
            thread::Builder::new()
                .name("interrupt".into())
                .spawn(move || {
                    interrupt::run_interrupt_detector(tx_interrupt, tp_phys_clone.as_deref());
                })?,
        )
    } else {
        log::info!("Interrupt detection disabled");
        None
    };

    log::info!("Momentum engine started (mode={})", args.mode);
    momentum::run_engine(rx, vdev, &args);

    let _ = touchpad_thread.join();
    if let Some(t) = interrupt_thread {
        let _ = t.join();
    }

    Ok(())
}

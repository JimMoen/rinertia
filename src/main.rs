use anyhow::Result;
use clap::Parser;
use log;
use std::sync::mpsc;
use std::thread;

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
    /// Touchpad device path (auto-detect if omitted)
    #[arg(short, long)]
    pub device: Option<String>,

    /// Match touchpad by name substring (e.g. "ELAN", "Synaptics")
    #[arg(short = 'n', long)]
    pub device_name: Option<String>,

    /// Operating mode
    #[arg(long, default_value = "scroll", value_parser = ["scroll", "pointer", "both"])]
    pub mode: String,

    /// Damping coefficient (0.0 ~ 1.0). Higher = more resistance = faster deceleration
    #[arg(long, default_value_t = 0.05)]
    pub damping: f64,

    /// Decay mode: "dual" = exponential then linear tail, "expo" = pure exponential
    #[arg(long, default_value = "dual", value_parser = ["dual", "expo"])]
    pub decay_mode: String,

    /// Velocity threshold to transition from exponential to linear phase (hires per 8ms, only for dual mode)
    #[arg(long, default_value_t = 360.0)]
    pub phase_threshold: f64,

    /// Duration of linear deceleration phase in ms (only for dual mode)
    #[arg(long, default_value_t = 384)]
    pub linear_decel_ms: i32,

    /// Linear phase stops when output reaches this value (hires units, only for dual mode)
    #[arg(long, default_value_t = 1)]
    pub linear_stop_hires: i32,

    /// Minimum velocity (hires/sec after tp_to_hires conversion) to trigger scroll momentum
    #[arg(long, default_value_t = 120.0)]
    pub min_scroll_velocity: f64,

    /// Scroll speed multiplier
    #[arg(long, default_value_t = 1.0)]
    pub scroll_factor: f64,

    /// Touchpad-to-hires conversion factor (device-specific, adjust if scroll is too fast/slow)
    #[arg(long, default_value_t = 5.0)]
    pub tp_to_hires: f64,

    /// Drag coefficient for pointer inertia (0.0 ~ 1.0)
    #[arg(long, default_value_t = 0.15)]
    pub pointer_drag: f64,

    /// Scale factor from touchpad units to virtual mouse units
    #[arg(long, default_value_t = 0.0075)]
    pub pointer_speed_factor: f64,

    /// Minimum touchpad speed to trigger pointer inertia
    #[arg(long, default_value_t = 2000.0)]
    pub pointer_min_velocity: f64,

    /// Multitouch cooldown in ms (prevents inertia after gestures)
    #[arg(long, default_value_t = 500)]
    pub multitouch_cooldown: u64,

    /// Disable keyboard/mouse interrupt detection
    #[arg(long)]
    pub no_interrupt: bool,

    /// Dry mode: don't create virtual device, only log
    #[arg(long)]
    pub dry: bool,

    /// Log level (off, error, warn, info, debug, trace)
    #[arg(long, default_value = "info")]
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
    let args = Args::parse();

    env_logger::Builder::new()
        .filter_module(
            "rinertia",
            args.log_level.parse().unwrap_or(log::LevelFilter::Info),
        )
        .parse_default_env()
        .init();

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

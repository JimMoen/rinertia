use std::process::Command;

pub fn detect(touchpad_sysname: &str) -> Option<bool> {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_lowercase();

    if desktop.contains("kde") || desktop.contains("plasma") {
        if let Some(val) = detect_kde(touchpad_sysname) {
            log::info!("Detected natural_scroll={} from KDE/KWin D-Bus", val);
            return Some(val);
        }
    }

    if desktop.contains("gnome") {
        if let Some(val) = detect_gnome() {
            log::info!("Detected natural_scroll={} from GNOME gsettings", val);
            return Some(val);
        }
    }

    // TODO: Sway — swaymsg -t get_inputs (JSON, find touchpad natural_scroll)
    // TODO: Hyprland — hyprctl -j getoption input:touchpad:natural_scroll
    // TODO: niri — parse ~/.config/niri/config.kdl touchpad block

    if let Some(val) = detect_libinput(touchpad_sysname) {
        log::warn!(
            "Could not detect desktop environment scroll setting. \
             Using libinput device default: natural_scroll={}. \
             If this is wrong, set --natural-scroll or natural_scroll in config.",
            val
        );
        return Some(val);
    }

    None
}

fn detect_kde(sysname: &str) -> Option<bool> {
    let path = format!("/org/kde/KWin/InputDevice/{}", sysname);
    let output = Command::new("busctl")
        .args([
            "--user",
            "get-property",
            "org.kde.KWin",
            &path,
            "org.kde.KWin.InputDevice",
            "naturalScroll",
        ])
        .output()
        .ok()?;
    // busctl output: "b true\n" or "b false\n"
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.starts_with("b ") {
        Some(stdout.trim().ends_with("true"))
    } else {
        None
    }
}

fn detect_gnome() -> Option<bool> {
    let output = Command::new("gsettings")
        .args([
            "get",
            "org.gnome.desktop.peripherals.touchpad",
            "natural-scroll",
        ])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    match stdout.as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn detect_libinput(sysname: &str) -> Option<bool> {
    let output = Command::new("libinput")
        .args(["list-devices"])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let kernel_line = format!("/dev/input/{}", sysname);
    let mut found_device = false;
    for line in stdout.lines() {
        if line.contains(&kernel_line) {
            found_device = true;
            continue;
        }
        if found_device {
            if line.contains("Nat.scrolling:") {
                return Some(line.contains("enabled"));
            }
            if line.starts_with("Device:") {
                break;
            }
        }
    }
    None
}

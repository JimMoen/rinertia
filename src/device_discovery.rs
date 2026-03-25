use anyhow::{bail, Result};
use std::path::PathBuf;

/// Find a touchpad device, either by explicit path, name filter, or auto-detection.
pub fn find_touchpad(device_path: Option<&str>, name_filter: Option<&str>) -> Result<PathBuf> {
    if let Some(path) = device_path {
        let p = PathBuf::from(path);
        if !p.exists() {
            bail!("Device path does not exist: {}", path);
        }
        return Ok(p);
    }

    if let Some(filter) = name_filter {
        for (path, device) in evdev::enumerate() {
            let name = device.name().unwrap_or("");
            log::debug!("Enumerating: {} [{}]", path.display(), name);
            if name.contains(filter) {
                log::info!(
                    "Matched device by name filter \"{}\": {} [{}]",
                    filter,
                    path.display(),
                    name
                );
                return Ok(path);
            }
        }
        bail!("No device found matching name filter: \"{}\"", filter);
    }

    let mut candidates: Vec<(PathBuf, String, bool)> = Vec::new();

    for (path, device) in evdev::enumerate() {
        let name = device.name().unwrap_or("").to_string();
        log::debug!("Enumerating: {} [{}]", path.display(), name);

        let keys = match device.supported_keys() {
            Some(k) => k,
            None => continue,
        };

        if !keys.contains(evdev::Key::BTN_TOOL_FINGER) || !keys.contains(evdev::Key::BTN_TOUCH) {
            continue;
        }

        let has_mt = device.supported_absolute_axes().map_or(false, |axes| {
            axes.contains(evdev::AbsoluteAxisType::ABS_MT_POSITION_X)
        });

        log::debug!(
            "Touchpad candidate: {} [{}] multitouch={}",
            path.display(),
            name,
            has_mt
        );
        candidates.push((path, name, has_mt));
    }

    if candidates.is_empty() {
        bail!("No touchpad device found (looked for BTN_TOOL_FINGER + BTN_TOUCH)");
    }

    candidates.sort_by(|a, b| b.2.cmp(&a.2));

    let (path, name, _) = candidates.into_iter().next().unwrap();
    log::info!("Auto-detected touchpad: {} [{}]", path.display(), name);
    Ok(path)
}

/// Get the physical path (sysfs location) of an evdev device.
pub fn get_phys(device: &evdev::Device) -> Option<String> {
    device.physical_path().map(|s| s.to_string())
}

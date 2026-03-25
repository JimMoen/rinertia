use crate::MomentumMessage;
use evdev::{Device, InputEventKind, Key, RelativeAxisType};
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

pub fn run_interrupt_detector(tx: mpsc::Sender<MomentumMessage>, touchpad_phys: Option<&str>) {
    let devices = find_interrupt_devices(touchpad_phys);
    if devices.is_empty() {
        log::warn!("No interrupt devices found");
        return;
    }

    let mut handles = Vec::new();
    for (path, mut device) in devices {
        let tx = tx.clone();
        let name = device.name().unwrap_or("?").to_string();
        log::info!("Interrupt monitor: {} [{}]", name, path.display());
        handles.push(
            thread::Builder::new()
                .name(format!("interrupt-{}", name))
                .spawn(move || {
                    monitor_device(&mut device, &tx, &name);
                }),
        );
    }

    for h in handles {
        if let Ok(h) = h {
            let _ = h.join();
        }
    }
}

fn find_interrupt_devices(touchpad_phys: Option<&str>) -> Vec<(PathBuf, Device)> {
    let mut result = Vec::new();

    let Ok(entries) = std::fs::read_dir("/dev/input") else {
        log::error!("Cannot read /dev/input");
        return result;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(fname) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !fname.starts_with("event") {
            continue;
        }

        let Ok(device) = Device::open(&path) else {
            continue;
        };

        let name = device.name().unwrap_or("");
        if name.contains("rinertia") {
            continue;
        }

        // Exclude the touchpad's own physical device — its auxiliary mouse interface
        // would otherwise trigger false interrupts during normal touchpad use.
        if let Some(tp_phys) = touchpad_phys {
            if let Some(phys) = device.physical_path() {
                if phys == tp_phys {
                    continue;
                }
            }
        }

        if is_keyboard(&device) || is_external_mouse(&device) {
            result.push((path, device));
        }
    }

    result
}

fn is_keyboard(device: &Device) -> bool {
    let Some(keys) = device.supported_keys() else {
        return false;
    };
    keys.contains(Key::KEY_A) && keys.contains(Key::KEY_Z)
}

fn is_external_mouse(device: &Device) -> bool {
    let keys = device.supported_keys();
    let has_btn_left = keys.as_ref().map_or(false, |k| k.contains(Key::BTN_LEFT));
    if !has_btn_left {
        return false;
    }

    let has_rel_x = device
        .supported_relative_axes()
        .map_or(false, |r| r.contains(RelativeAxisType::REL_X));
    if !has_rel_x {
        return false;
    }

    let is_touchpad = keys
        .as_ref()
        .map_or(false, |k| k.contains(Key::BTN_TOOL_FINGER));
    !is_touchpad
}

fn monitor_device(device: &mut Device, tx: &mpsc::Sender<MomentumMessage>, name: &str) {
    loop {
        match device.fetch_events() {
            Ok(events) => {
                for event in events {
                    if is_interrupt_event(&event) {
                        if tx.send(MomentumMessage::Stop).is_err() {
                            log::debug!("Interrupt channel closed for {}", name);
                            return;
                        }
                    }
                }
            }
            Err(e) => {
                log::error!("Error reading {}: {}", name, e);
                break;
            }
        }
    }
}

fn is_interrupt_event(event: &evdev::InputEvent) -> bool {
    match event.kind() {
        InputEventKind::Key(key) => {
            if event.value() != 1 {
                return false;
            }
            matches!(key, Key::BTN_LEFT | Key::BTN_RIGHT | Key::BTN_MIDDLE)
                || (key.code() >= Key::KEY_ESC.code() && key.code() <= Key::KEY_MICMUTE.code())
        }
        InputEventKind::RelAxis(axis) => {
            matches!(axis, RelativeAxisType::REL_X | RelativeAxisType::REL_Y)
        }
        _ => false,
    }
}

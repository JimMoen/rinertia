use crate::MomentumMessage;
use evdev::{Device, InputEventKind, Key, RelativeAxisType};
use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use std::os::fd::{AsRawFd, BorrowedFd};
use std::path::PathBuf;
use std::sync::mpsc;

pub fn run_interrupt_detector(tx: mpsc::Sender<MomentumMessage>, touchpad_phys: Option<&str>) {
    let mut devices = find_interrupt_devices(touchpad_phys);
    if devices.is_empty() {
        log::warn!("No interrupt devices found");
        return;
    }

    for (path, device) in &devices {
        log::info!(
            "Interrupt monitor: {} [{}]",
            device.name().unwrap_or("?"),
            path.display()
        );
    }

    monitor_devices(&mut devices, &tx);
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
        if let Some(tp_phys) = touchpad_phys
            && let Some(phys) = device.physical_path()
            && phys == tp_phys
        {
            continue;
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
    let has_btn_left = keys.as_ref().is_some_and(|k| k.contains(Key::BTN_LEFT));
    if !has_btn_left {
        return false;
    }

    let has_rel_x = device
        .supported_relative_axes()
        .is_some_and(|r| r.contains(RelativeAxisType::REL_X));
    if !has_rel_x {
        return false;
    }

    let is_touchpad = keys
        .as_ref()
        .is_some_and(|k| k.contains(Key::BTN_TOOL_FINGER));
    !is_touchpad
}

// Single-threaded poll loop over all keyboards/mice; replaces the previous
// one-thread-per-device design (a desktop can have half a dozen such devices).
fn monitor_devices(devices: &mut Vec<(PathBuf, Device)>, tx: &mpsc::Sender<MomentumMessage>) {
    loop {
        let mut poll_fds: Vec<PollFd> = devices
            .iter()
            .map(|(_, d)| {
                // SAFETY: the fd is owned by `d`, which outlives the poll call.
                let fd = unsafe { BorrowedFd::borrow_raw(d.as_raw_fd()) };
                PollFd::new(fd, PollFlags::POLLIN)
            })
            .collect();

        match poll(&mut poll_fds, PollTimeout::NONE) {
            Ok(_) => {}
            Err(nix::errno::Errno::EINTR) => continue,
            Err(e) => {
                log::error!("Interrupt poll failed: {}", e);
                return;
            }
        }

        let mut stop = false;
        let mut remove_idx: Option<usize> = None;

        for (i, pfd) in poll_fds.iter().enumerate() {
            let Some(revents) = pfd.revents() else {
                continue;
            };

            if revents.intersects(PollFlags::POLLERR | PollFlags::POLLHUP) {
                log::error!(
                    "Interrupt device gone: {} [{}]",
                    devices[i].1.name().unwrap_or("?"),
                    devices[i].0.display()
                );
                remove_idx = Some(i);
                break;
            }
            if !revents.contains(PollFlags::POLLIN) {
                continue;
            }

            let (interrupted, read_err) = {
                let device = &mut devices[i].1;
                match device.fetch_events() {
                    Ok(events) => {
                        let interrupted = events.into_iter().any(|e| is_interrupt_event(&e));
                        (interrupted, None)
                    }
                    Err(e) => (false, Some(e)),
                }
            };

            if let Some(e) = read_err {
                log::error!("Error reading {}: {}", devices[i].0.display(), e);
                remove_idx = Some(i);
                break;
            }
            if interrupted && tx.send(MomentumMessage::Stop).is_err() {
                log::debug!("Interrupt channel closed");
                stop = true;
                break;
            }
        }

        if stop {
            return;
        }
        if let Some(i) = remove_idx {
            devices.swap_remove(i);
            if devices.is_empty() {
                log::warn!("No interrupt devices left");
                return;
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

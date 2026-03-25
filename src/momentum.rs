use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::virtual_device::VirtualDevice;
use crate::{MomentumMessage, ScrollAxis};

#[derive(Debug, Clone, Copy, PartialEq)]
enum EngineState {
    Idle,
    ScrollMomentum,
    ScrollLinearDecel,
    PointerMomentum,
}

pub fn run_engine(
    rx: mpsc::Receiver<MomentumMessage>,
    mut vdev: Option<VirtualDevice>,
    args: &crate::Args,
) {
    let retention = (1.0 - args.damping.clamp(0.0, 0.99)) as f64;
    let dual_phase = args.decay_mode == "dual";
    let phase_threshold = args.phase_threshold;
    let linear_decel_ms = args.linear_decel_ms as f64;
    let linear_stop_hires = args.linear_stop_hires;
    let scroll_factor = args.scroll_factor;
    let pointer_drag = args.pointer_drag;
    let pointer_speed_factor = args.pointer_speed_factor;

    let mut state = EngineState::Idle;
    let mut velocity: f64 = 0.0;
    let mut axis = ScrollAxis::Vertical;
    let mut last_tick = Instant::now();

    let mut linear_decel_rate: f64 = 0.0;

    let mut vx: f64 = 0.0;
    let mut vy: f64 = 0.0;

    loop {
        match state {
            EngineState::Idle => {
                let msg = match rx.recv() {
                    Ok(m) => m,
                    Err(_) => {
                        log::info!("Channel closed, engine shutting down");
                        return;
                    }
                };
                match msg {
                    MomentumMessage::StartScroll {
                        velocity_hires_per_sec,
                        axis: msg_axis,
                    } => {
                        velocity = velocity_hires_per_sec;
                        axis = msg_axis;
                        state = EngineState::ScrollMomentum;
                        last_tick = Instant::now();
                        log::debug!(
                            "ScrollMomentum start: velocity={:.1} axis={:?}",
                            velocity,
                            axis
                        );
                    }
                    MomentumMessage::StartPointer { vx: pvx, vy: pvy } => {
                        vx = pvx;
                        vy = pvy;
                        state = EngineState::PointerMomentum;
                        last_tick = Instant::now();
                        log::debug!("PointerMomentum start: vx={:.1} vy={:.1}", vx, vy);
                    }
                    MomentumMessage::Stop => {}
                }
            }

            // Dual-phase scroll momentum ported from waynaptics ScrollInterceptor:
            //   Phase 1 (ScrollMomentum): Exponential decay — velocity is multiplied by
            //   retention^(dt/16) each tick, where retention = 1.0 - damping.
            //   Phase 2 (ScrollLinearDecel): When velocity drops below a threshold, switch
            //   to constant deceleration rate for a clean stop without asymptotic crawl.
            EngineState::ScrollMomentum => {
                let msg = rx.recv_timeout(Duration::from_millis(1));
                match msg {
                    Ok(MomentumMessage::Stop) => {
                        log::debug!("ScrollMomentum interrupted by Stop");
                        velocity = 0.0;
                        state = EngineState::Idle;
                        continue;
                    }
                    Ok(MomentumMessage::StartScroll {
                        velocity_hires_per_sec,
                        axis: msg_axis,
                    }) => {
                        let same_direction =
                            msg_axis == axis && (velocity_hires_per_sec > 0.0) == (velocity > 0.0);
                        if same_direction {
                            log::debug!("Same-direction scroll during momentum, going Idle");
                        } else {
                            log::debug!("Opposite-direction scroll during momentum, stopping");
                        }
                        velocity = 0.0;
                        state = EngineState::Idle;
                        continue;
                    }
                    Ok(MomentumMessage::StartPointer { .. }) => {}
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        log::info!("Channel closed, engine shutting down");
                        return;
                    }
                }

                let dt_ms = last_tick.elapsed().as_secs_f64() * 1000.0;
                last_tick = Instant::now();

                // Time-corrected damping: normalize to 16ms reference frame so decay
                // rate is independent of actual tick interval.
                velocity *= retention.powf(dt_ms / 16.0);

                let delta_hires = velocity * (dt_ms / 1000.0) * scroll_factor;
                let emit_hires = delta_hires.round() as i32;

                if dual_phase {
                    let hires_per_8ms = velocity.abs() * (8.0 / 1000.0) * scroll_factor;
                    if hires_per_8ms < phase_threshold {
                        linear_decel_rate = velocity.abs() / (linear_decel_ms / 1000.0);
                        state = EngineState::ScrollLinearDecel;
                        last_tick = Instant::now();
                        log::debug!(
                            "Transition to LinearDecel: velocity={:.1} decel_rate={:.1}",
                            velocity,
                            linear_decel_rate
                        );
                        continue;
                    }
                } else if emit_hires == 0 {
                    log::debug!("Expo decay: velocity decayed to zero output");
                    velocity = 0.0;
                    state = EngineState::Idle;
                    continue;
                }

                if emit_hires != 0 {
                    emit_scroll(&mut vdev, axis, emit_hires);
                }
            }

            EngineState::ScrollLinearDecel => {
                let msg = rx.recv_timeout(Duration::from_millis(1));
                match msg {
                    Ok(MomentumMessage::Stop) => {
                        log::debug!("LinearDecel interrupted by Stop");
                        velocity = 0.0;
                        state = EngineState::Idle;
                        continue;
                    }
                    Ok(MomentumMessage::StartScroll { .. }) => {
                        log::debug!("New scroll during LinearDecel, going Idle");
                        velocity = 0.0;
                        state = EngineState::Idle;
                        continue;
                    }
                    Ok(MomentumMessage::StartPointer { .. }) => {}
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        log::info!("Channel closed, engine shutting down");
                        return;
                    }
                }

                let dt_ms = last_tick.elapsed().as_secs_f64() * 1000.0;
                last_tick = Instant::now();

                let sign = if velocity > 0.0 { 1.0 } else { -1.0 };
                velocity -= sign * linear_decel_rate * (dt_ms / 1000.0);

                if sign * velocity <= 0.0 {
                    log::debug!("LinearDecel: velocity crossed zero, going Idle");
                    velocity = 0.0;
                    state = EngineState::Idle;
                    continue;
                }

                let delta_hires = velocity * (dt_ms / 1000.0) * scroll_factor;
                let mut emit_hires = delta_hires.round() as i32;
                if emit_hires == 0 {
                    emit_hires = if velocity > 0.0 { 1 } else { -1 };
                }

                if emit_hires.abs() <= linear_stop_hires {
                    log::debug!(
                        "LinearDecel stop: |hires|={} <= {}",
                        emit_hires.abs(),
                        linear_stop_hires
                    );
                    velocity = 0.0;
                    state = EngineState::Idle;
                    continue;
                }

                emit_scroll(&mut vdev, axis, emit_hires);
            }

            EngineState::PointerMomentum => {
                let msg = rx.recv_timeout(Duration::from_secs_f64(1.0 / 60.0));
                match msg {
                    Ok(MomentumMessage::Stop) => {
                        log::debug!("PointerMomentum interrupted by Stop");
                        vx = 0.0;
                        vy = 0.0;
                        state = EngineState::Idle;
                        continue;
                    }
                    Ok(MomentumMessage::StartScroll { .. }) => {}
                    Ok(MomentumMessage::StartPointer {
                        vx: new_vx,
                        vy: new_vy,
                    }) => {
                        vx = new_vx;
                        vy = new_vy;
                        last_tick = Instant::now();
                        continue;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        log::info!("Channel closed, engine shutting down");
                        return;
                    }
                }

                let decel_factor = 1.0 - pointer_drag;
                vx *= decel_factor;
                vy *= decel_factor;
                last_tick = Instant::now();

                let dx = (vx * pointer_speed_factor).round() as i32;
                let dy = (vy * pointer_speed_factor).round() as i32;

                if dx == 0 && dy == 0 {
                    log::debug!("PointerMomentum: velocity decayed to zero");
                    vx = 0.0;
                    vy = 0.0;
                    state = EngineState::Idle;
                    continue;
                }

                emit_pointer(&mut vdev, dx, dy);
            }
        }
    }
}

fn emit_scroll(vdev: &mut Option<VirtualDevice>, axis: ScrollAxis, hires: i32) {
    match vdev {
        Some(dev) => {
            if let Err(e) = dev.emit_scroll(axis, hires) {
                log::error!("emit_scroll failed: {}", e);
            }
        }
        None => {
            log::info!("[dry] scroll {:?} hires={}", axis, hires);
        }
    }
}

fn emit_pointer(vdev: &mut Option<VirtualDevice>, dx: i32, dy: i32) {
    match vdev {
        Some(dev) => {
            if let Err(e) = dev.emit_pointer(dx, dy) {
                log::error!("emit_pointer failed: {}", e);
            }
        }
        None => {
            log::info!("[dry] pointer dx={} dy={}", dx, dy);
        }
    }
}

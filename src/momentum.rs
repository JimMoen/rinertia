use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::virtual_device::VirtualDevice;
use crate::{MomentumMessage, ScrollAxis};

const SCROLL_TICK_MS: u64 = 8;

#[derive(Debug, Clone, Copy, PartialEq)]
enum EngineState {
    Idle,
    ScrollMomentum,
    ScrollLinearDecel,
    ScrollTailEmit,
    ScrollMacos,
    PointerMomentum,
}

pub fn run_engine(
    rx: mpsc::Receiver<MomentumMessage>,
    mut vdev: Option<VirtualDevice>,
    args: &crate::ResolvedArgs,
) {
    let damping_curve = args.damping_curve.as_str();
    let retention = (1.0 - args.damping.clamp(0.0, 0.99)) as f64;
    let phase_threshold = args.phase_threshold;
    let linear_decel_ms = args.linear_decel_ms as f64;
    let linear_stop_hires = args.linear_stop_hires;
    let time_constant_ms = args.time_constant_ms;
    let stop_threshold = args.stop_threshold;
    let tail_scroll_ms = args.tail_scroll_ms as f64;
    let scroll_factor = args.scroll_factor;
    let pointer_drag = args.pointer_drag;
    let pointer_speed_factor = args.pointer_speed_factor;

    let mut state = EngineState::Idle;
    let mut velocity: f64 = 0.0;
    let mut axis = ScrollAxis::Vertical;
    let mut last_tick = Instant::now();
    let mut hires_accumulator: f64 = 0.0;

    let mut linear_decel_rate: f64 = 0.0;
    let mut tail_sign: i32 = 1;
    let mut tail_elapsed_ms: f64 = 0.0;

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
                        hires_accumulator = 0.0;
                        state = match damping_curve {
                            "macos" => EngineState::ScrollMacos,
                            _ => EngineState::ScrollMomentum,
                        };
                        last_tick = Instant::now();
                        log::debug!(
                            "{:?} start: velocity={:.1} axis={:?}",
                            state,
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

            EngineState::ScrollMomentum => {
                let msg = rx.recv_timeout(Duration::from_millis(SCROLL_TICK_MS));
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

                velocity *= retention.powf(dt_ms / 16.0);

                hires_accumulator += velocity * (dt_ms / 1000.0) * scroll_factor;
                let emit_hires = hires_accumulator.trunc() as i32;
                hires_accumulator -= emit_hires as f64;

                if damping_curve == "dual" {
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
                        if emit_hires != 0 {
                            emit_scroll(&mut vdev, axis, emit_hires);
                        }
                        continue;
                    }
                } else if emit_hires == 0 && velocity.abs() < stop_threshold {
                    log::debug!("Expo decay: velocity below threshold");
                    enter_tail_or_idle(
                        &mut state,
                        &mut velocity,
                        &mut tail_sign,
                        &mut tail_elapsed_ms,
                        &mut last_tick,
                        tail_scroll_ms,
                    );
                    continue;
                }

                if emit_hires != 0 {
                    emit_scroll(&mut vdev, axis, emit_hires);
                    log::debug!(
                        "ExpoDecay emit: hires={} vel={:.1} dt={:.2}ms",
                        emit_hires,
                        velocity,
                        dt_ms
                    );
                }
            }

            EngineState::ScrollLinearDecel => {
                let msg = rx.recv_timeout(Duration::from_millis(SCROLL_TICK_MS));
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
                    log::debug!("LinearDecel: velocity crossed zero");
                    velocity = 0.0;
                    enter_tail_or_idle(
                        &mut state,
                        &mut velocity,
                        &mut tail_sign,
                        &mut tail_elapsed_ms,
                        &mut last_tick,
                        tail_scroll_ms,
                    );
                    if tail_scroll_ms > 0.0 {
                        tail_sign = if sign > 0.0 { 1 } else { -1 };
                    }
                    continue;
                }

                hires_accumulator += velocity * (dt_ms / 1000.0) * scroll_factor;
                let emit_hires = hires_accumulator.trunc() as i32;
                hires_accumulator -= emit_hires as f64;

                if emit_hires.abs() <= linear_stop_hires && emit_hires != 0 {
                    log::debug!(
                        "LinearDecel stop: |hires|={} <= {}",
                        emit_hires.abs(),
                        linear_stop_hires
                    );
                    emit_scroll(&mut vdev, axis, emit_hires);
                    let dir = if emit_hires > 0 { 1 } else { -1 };
                    velocity = 0.0;
                    enter_tail_or_idle(
                        &mut state,
                        &mut velocity,
                        &mut tail_sign,
                        &mut tail_elapsed_ms,
                        &mut last_tick,
                        tail_scroll_ms,
                    );
                    if tail_scroll_ms > 0.0 {
                        tail_sign = dir;
                    }
                    continue;
                }

                if emit_hires != 0 {
                    emit_scroll(&mut vdev, axis, emit_hires);
                    log::debug!(
                        "LinearDecel emit: hires={} vel={:.1} dt={:.2}ms",
                        emit_hires,
                        velocity,
                        dt_ms
                    );
                }
            }

            // v(t) = v₀ × exp(-t / τ)
            EngineState::ScrollMacos => {
                let msg = rx.recv_timeout(Duration::from_millis(SCROLL_TICK_MS));
                match msg {
                    Ok(MomentumMessage::Stop) => {
                        log::debug!("ScrollMacos interrupted by Stop");
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
                            log::debug!("Same-direction scroll during macos momentum, going Idle");
                        } else {
                            log::debug!(
                                "Opposite-direction scroll during macos momentum, stopping"
                            );
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

                velocity *= (-dt_ms / time_constant_ms).exp();

                if velocity.abs() < stop_threshold {
                    log::debug!(
                        "ScrollMacos stop: |velocity|={:.1} < {:.1}",
                        velocity.abs(),
                        stop_threshold
                    );
                    enter_tail_or_idle(
                        &mut state,
                        &mut velocity,
                        &mut tail_sign,
                        &mut tail_elapsed_ms,
                        &mut last_tick,
                        tail_scroll_ms,
                    );
                    continue;
                }

                hires_accumulator += velocity * (dt_ms / 1000.0) * scroll_factor;
                let emit_hires = hires_accumulator.trunc() as i32;
                hires_accumulator -= emit_hires as f64;

                if emit_hires != 0 {
                    emit_scroll(&mut vdev, axis, emit_hires);
                }
            }

            EngineState::ScrollTailEmit => {
                let msg = rx.recv_timeout(Duration::from_millis(SCROLL_TICK_MS));
                match msg {
                    Ok(MomentumMessage::Stop) => {
                        log::debug!("TailEmit interrupted by Stop");
                        state = EngineState::Idle;
                        continue;
                    }
                    Ok(MomentumMessage::StartScroll { .. }) => {
                        log::debug!("New scroll during TailEmit, going Idle");
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
                tail_elapsed_ms += dt_ms;

                if tail_elapsed_ms >= tail_scroll_ms {
                    log::debug!("TailEmit finished: {:.0}ms elapsed", tail_elapsed_ms);
                    state = EngineState::Idle;
                    continue;
                }

                emit_scroll(&mut vdev, axis, tail_sign);
                log::debug!(
                    "TailEmit emit: hires={} elapsed={:.0}ms dt={:.2}ms",
                    tail_sign,
                    tail_elapsed_ms,
                    dt_ms
                );
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

fn enter_tail_or_idle(
    state: &mut EngineState,
    velocity: &mut f64,
    tail_sign: &mut i32,
    tail_elapsed_ms: &mut f64,
    last_tick: &mut Instant,
    tail_scroll_ms: f64,
) {
    let sign = if *velocity > 0.0 { 1 } else { -1 };
    *velocity = 0.0;
    if tail_scroll_ms > 0.0 {
        *tail_sign = sign;
        *tail_elapsed_ms = 0.0;
        *state = EngineState::ScrollTailEmit;
        *last_tick = Instant::now();
        log::debug!("-> TailEmit: {}ms, sign={}", tail_scroll_ms, *tail_sign);
    } else {
        *state = EngineState::Idle;
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

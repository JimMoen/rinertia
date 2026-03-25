use evdev::{AbsoluteAxisType, InputEventKind, Key};
use std::sync::mpsc;
use std::time;

use crate::{MomentumMessage, ResolvedArgs, ScrollAxis};

const RING_SIZE: usize = 8;
const VELOCITY_SAMPLES: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq)]
enum ListenerState {
    Idle,
    OneFingerMove,
    TwoFingerScroll,
}

struct RingBuffer {
    buf: [(f64, u64); RING_SIZE],
    pos: usize,
    count: usize,
}

impl RingBuffer {
    fn new() -> Self {
        Self {
            buf: [(0.0, 0); RING_SIZE],
            pos: 0,
            count: 0,
        }
    }

    fn push(&mut self, delta: f64, timestamp_us: u64) {
        self.buf[self.pos] = (delta, timestamp_us);
        self.pos = (self.pos + 1) % RING_SIZE;
        if self.count < RING_SIZE {
            self.count += 1;
        }
    }

    fn clear(&mut self) {
        self.count = 0;
        self.pos = 0;
    }

    fn compute_velocity(&self, tp_to_hires: f64) -> f64 {
        let n = self.count.min(VELOCITY_SAMPLES);
        if n < 2 {
            return 0.0;
        }

        let start = if self.pos >= n {
            self.pos - n
        } else {
            RING_SIZE - (n - self.pos)
        };

        let mut total_delta = 0.0;
        let first_ts = self.buf[start].1;
        let mut last_ts = first_ts;

        for i in 1..n {
            let idx = (start + i) % RING_SIZE;
            total_delta += self.buf[idx].0;
            last_ts = self.buf[idx].1;
        }

        let dt_us = last_ts.saturating_sub(first_ts);
        if dt_us == 0 {
            return 0.0;
        }

        let dt_sec = dt_us as f64 / 1_000_000.0;
        (total_delta / dt_sec) * tp_to_hires
    }
}

fn timestamp_to_us(ts: time::SystemTime) -> u64 {
    ts.duration_since(time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}

pub fn run_listener(
    mut device: evdev::Device,
    tx: mpsc::Sender<MomentumMessage>,
    args: &ResolvedArgs,
) {
    let enable_scroll = args.mode == "scroll" || args.mode == "both";
    let enable_pointer = args.mode == "pointer" || args.mode == "both";
    let multitouch_cooldown_us = args.multitouch_cooldown * 1000;

    let mut state = ListenerState::Idle;

    let mut scroll_ring_y = RingBuffer::new();
    let mut scroll_ring_x = RingBuffer::new();

    let mut ptr_x: i32 = 0;
    let mut ptr_y: i32 = 0;
    let mut ptr_prev_x: i32 = 0;
    let mut ptr_prev_y: i32 = 0;
    let mut ptr_vx: f64 = 0.0;
    let mut ptr_vy: f64 = 0.0;
    let mut ptr_prev_ts = time::SystemTime::UNIX_EPOCH;

    let mut scroll_prev_y: i32 = 0;
    let mut scroll_prev_x: i32 = 0;

    let mut multitouch_ts: u64 = 0;
    let mut current_ts = time::SystemTime::UNIX_EPOCH;

    while let Ok(events) = device.fetch_events() {
        for event in events {
            current_ts = event.timestamp();
            log::trace!("Event: {:?} = {}", event.kind(), event.value());

            match event.kind() {
                InputEventKind::AbsAxis(axis) => match axis {
                    AbsoluteAxisType::ABS_X => {
                        let val = event.value();
                        match state {
                            ListenerState::OneFingerMove => {
                                ptr_x = val;
                            }
                            ListenerState::TwoFingerScroll => {
                                let ts_us = timestamp_to_us(current_ts);
                                let delta = (val - scroll_prev_x) as f64;
                                scroll_ring_x.push(delta, ts_us);
                                scroll_prev_x = val;
                            }
                            ListenerState::Idle => {}
                        }
                    }
                    AbsoluteAxisType::ABS_Y => {
                        let val = event.value();
                        match state {
                            ListenerState::OneFingerMove => {
                                ptr_y = val;
                            }
                            ListenerState::TwoFingerScroll => {
                                let ts_us = timestamp_to_us(current_ts);
                                let delta = (val - scroll_prev_y) as f64;
                                scroll_ring_y.push(delta, ts_us);
                                scroll_prev_y = val;
                            }
                            ListenerState::Idle => {}
                        }
                    }
                    _ => {}
                },
                InputEventKind::Key(key) => match key {
                    Key::BTN_TOOL_FINGER => {
                        if event.value() == 1 {
                            // BTN_TOOL_FINGER=1 fires when:
                            //   a) First finger touches (Idle → OneFingerMove)
                            //   b) Second finger lifts during two-finger scroll
                            //      (BTN_TOOL_DOUBLETAP=0 + BTN_TOOL_FINGER=1 in same frame)
                            // Case (b) is handled by BTN_TOOL_DOUBLETAP=0 already computing
                            // velocity and transitioning to Idle. Don't interrupt here.
                            if state == ListenerState::Idle {
                                state = ListenerState::OneFingerMove;
                                ptr_prev_x = ptr_x;
                                ptr_prev_y = ptr_y;
                                ptr_vx = 0.0;
                                ptr_vy = 0.0;
                                ptr_prev_ts = current_ts;
                                log::debug!("State -> OneFingerMove");
                            }
                        } else {
                            if state == ListenerState::OneFingerMove && enable_pointer {
                                let now_us = timestamp_to_us(current_ts);
                                let mt_elapsed = now_us.saturating_sub(multitouch_ts);
                                if mt_elapsed < multitouch_cooldown_us {
                                    log::debug!("Pointer lift ignored (multitouch cooldown)");
                                    state = ListenerState::Idle;
                                    continue;
                                }
                                let speed = (ptr_vx * ptr_vx + ptr_vy * ptr_vy).sqrt();
                                if speed >= args.pointer_min_velocity {
                                    log::debug!(
                                        "Pointer inertia: vx={:.1} vy={:.1} speed={:.1}",
                                        ptr_vx,
                                        ptr_vy,
                                        speed
                                    );
                                    let _ = tx.send(MomentumMessage::StartPointer {
                                        vx: ptr_vx,
                                        vy: ptr_vy,
                                    });
                                }
                            }
                            state = ListenerState::Idle;
                        }
                    }
                    Key::BTN_TOOL_DOUBLETAP => {
                        if event.value() == 1 {
                            let _ = tx.send(MomentumMessage::Stop);
                            state = ListenerState::TwoFingerScroll;
                            scroll_ring_y.clear();
                            scroll_ring_x.clear();
                            scroll_prev_y = ptr_y;
                            scroll_prev_x = ptr_x;
                            log::debug!("State -> TwoFingerScroll");
                        } else {
                            if state == ListenerState::TwoFingerScroll && enable_scroll {
                                let vel_y = scroll_ring_y.compute_velocity(args.tp_to_hires);
                                let vel_x = scroll_ring_x.compute_velocity(args.tp_to_hires);
                                let abs_vy = vel_y.abs();
                                let abs_vx = vel_x.abs();

                                // ABS_Y increases downward, but REL_WHEEL_HI_RES positive
                                // means scroll up. Negate Y to match natural scroll direction.
                                // X axis: ABS_X increases rightward, REL_HWHEEL_HI_RES
                                // positive means scroll right — same direction, no negate.
                                let (velocity, axis) = if abs_vy >= abs_vx {
                                    (-vel_y * args.scroll_factor, ScrollAxis::Vertical)
                                } else {
                                    (vel_x * args.scroll_factor, ScrollAxis::Horizontal)
                                };

                                if velocity.abs() >= args.min_scroll_velocity {
                                    log::debug!(
                                        "Scroll momentum: vel={:.1} axis={:?}",
                                        velocity,
                                        axis
                                    );
                                    let _ = tx.send(MomentumMessage::StartScroll {
                                        velocity_hires_per_sec: velocity,
                                        axis,
                                    });
                                } else {
                                    log::debug!(
                                        "Scroll too slow: vel_y={:.1} vel_x={:.1} (threshold={})",
                                        vel_y,
                                        vel_x,
                                        args.min_scroll_velocity
                                    );
                                }
                            }
                            multitouch_ts = timestamp_to_us(current_ts);
                            state = ListenerState::Idle;
                        }
                    }
                    Key::BTN_TOOL_TRIPLETAP | Key::BTN_TOOL_QUADTAP | Key::BTN_TOOL_QUINTTAP => {
                        if event.value() == 1 {
                            let _ = tx.send(MomentumMessage::Stop);
                            state = ListenerState::Idle;
                            log::debug!("Multitouch gesture -> Stop");
                        } else {
                            multitouch_ts = timestamp_to_us(current_ts);
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        if state == ListenerState::OneFingerMove && enable_pointer {
            if ptr_x != ptr_prev_x || ptr_y != ptr_prev_y {
                let dt = current_ts
                    .duration_since(ptr_prev_ts)
                    .unwrap_or_default()
                    .as_secs_f64();
                if dt > 0.0 {
                    ptr_vx = (ptr_x - ptr_prev_x) as f64 / dt;
                    ptr_vy = (ptr_y - ptr_prev_y) as f64 / dt;
                }
                ptr_prev_x = ptr_x;
                ptr_prev_y = ptr_y;
                ptr_prev_ts = current_ts;
            }
        }
    }

    log::info!("Touchpad event stream ended");
}

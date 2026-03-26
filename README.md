# rinertia

Momentum scrolling for Linux laptop touchpads.

The Synaptics input driver used to provide momentum (coasting) scrolling — you could flick two fingers and the page would keep gliding. When the Linux desktop moved to libinput, this feature was dropped. rinertia brings it back as a standalone userspace daemon, working system-wide with any Wayland compositor and any application, zero configuration required.

## How it works

```
Kernel evdev ──► rinertia (passive read) ──► momentum engine ──► uinput virtual device
                     │                                                    │
                     │  (no GRAB, no libinput dependency)                 │
                     ▼                                                    ▼
              Touchpad events                                  REL_WHEEL_HI_RES
              still flow normally                              injected into
              to libinput / compositor                         the input stack
```

rinertia reads raw touchpad events directly from `/dev/input/eventN` via evdev — it does **not** depend on or interfere with libinput. Since it never grabs the device, your compositor, libinput, and gesture tools like [fusuma](https://github.com/iberianpig/fusuma) continue to work exactly as before.

## Features

- **Plug and play** — auto-detects your touchpad, just run it
- **Non-invasive** — passively reads events, never grabs your touchpad
- **Universal** — works with GTK, Qt, Electron, Firefox, and everything else
- **Interruptible** — touch the pad, press a key, or move your mouse to stop immediately
- **Tunable** — damping, decay curve, and speed are all adjustable

## Install

```bash
cargo build --release
```

## Usage

```bash
# Just works — auto-detect touchpad, default settings
sudo rinertia

# Match touchpad by name
sudo rinertia -n "ELAN"

# Longer, smoother inertia
sudo rinertia --damping 0.03 --linear-decel-ms 500

# Shorter, snappier inertia
sudo rinertia --damping 0.10 --linear-decel-ms 200

# Pure exponential decay (no linear tail)
sudo rinertia --damping-curve expo

# Troubleshooting — log only, no virtual device
sudo rinertia --dry --log-level debug

# See all options
rinertia --help
```

## Tuning

| Parameter | Effect of increasing |
|-----------|---------------------|
| `--damping` | Faster deceleration, shorter inertia |
| `--tp-to-hires` | More scroll distance per gesture |
| `--linear-decel-ms` | Slower, longer tail |
| `--scroll-factor` | More scroll output per tick |

## Known issues

- **Chromium-based browsers** have built-in smooth scrolling that may stack with rinertia. Disable it via `chrome://flags/#smooth-scrolling`, or use `--scroll-factor` to compensate.
- `--tp-to-hires` is device-specific — if scrolling feels too fast or slow, adjust this first.
- Pointer inertia (`--mode pointer`) is experimental.

## Acknowledgements

- [fusuma](https://github.com/iberianpig/fusuma) — multitouch gesture recognizer, whose architecture inspired our passive evdev listener design
- [waynaptics](https://github.com/kekekeks/waynaptics) — Wayland synaptics driver shim, from which the dual-phase momentum engine was originally ported
- [xkeysnail](https://github.com/mooz/xkeysnail) — evdev-based key remapper, whose passive evdev monitoring approach informed our no-GRAB design

## License

MIT

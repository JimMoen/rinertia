# rinertia

macOS-like inertial scrolling for Linux touchpads.

Lift your fingers after a two-finger scroll and the page keeps gliding — just like on a Mac. Works system-wide with any Wayland compositor, any application, zero configuration required.

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
sudo rinertia --decay-mode expo

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

## License

MIT

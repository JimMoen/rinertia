#!/usr/bin/env python3
"""Plot rinertia damping curves for visual comparison.

Usage:
    python3 tools/plot_curves.py                      # default output: damping_curves.png
    python3 tools/plot_curves.py -o curves.png        # custom output
    python3 tools/plot_curves.py --v0 50000           # custom initial velocity
    python3 tools/plot_curves.py --dual 0.05,60,384   # add custom dual curve (damping,threshold,linear_ms)
"""

import argparse
import sys

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np

TICK_MS = 8.0
TOTAL_MS = 5000
SCROLL_FACTOR = 1.0


def sim_dual(
    v0, damping, phase_threshold, linear_decel_ms, linear_stop_hires=1, tail_ms=0
):
    retention = 1.0 - damping
    ts, vs = [], []
    vel = v0
    acc = 0.0
    phase = "expo"
    ldr = 0.0
    sign_dir = 1.0
    for t in np.arange(0, TOTAL_MS, TICK_MS):
        ts.append(t)
        vs.append(vel)
        if phase == "expo":
            vel *= retention ** (TICK_MS / 16.0)
            acc += vel * (TICK_MS / 1000.0) * SCROLL_FACTOR
            emit = int(acc)
            acc -= emit
            if abs(vel) * (8.0 / 1000.0) < phase_threshold:
                ldr = abs(vel) / (linear_decel_ms / 1000.0)
                phase = "linear"
        elif phase == "linear":
            sign = 1.0 if vel > 0 else -1.0
            vel -= sign * ldr * (TICK_MS / 1000.0)
            if sign * vel <= 0:
                if tail_ms > 0:
                    phase = "tail"
                    tail_start = t
                    sign_dir = sign
                    vel = sign_dir * 1.0 / (TICK_MS / 1000.0)
                else:
                    break
                continue
            acc += vel * (TICK_MS / 1000.0) * SCROLL_FACTOR
            emit = int(acc)
            acc -= emit
            if abs(emit) <= linear_stop_hires and emit != 0:
                if tail_ms > 0:
                    phase = "tail"
                    tail_start = t
                    sign_dir = 1.0 if vel > 0 else -1.0
                    vel = sign_dir * 1.0 / (TICK_MS / 1000.0)
                else:
                    break
                continue
        elif phase == "tail":
            if t - tail_start >= tail_ms:
                ts.append(t + TICK_MS)
                vs.append(0)
                break
    return ts, vs


def sim_expo(v0, damping):
    retention = 1.0 - damping
    ts, vs = [], []
    vel = v0
    acc = 0.0
    for t in np.arange(0, TOTAL_MS, TICK_MS):
        ts.append(t)
        vs.append(vel)
        vel *= retention ** (TICK_MS / 16.0)
        acc += vel * (TICK_MS / 1000.0) * SCROLL_FACTOR
        emit = int(acc)
        acc -= emit
        if emit == 0 and abs(vel) < 60.0:
            break
    return ts, vs


def sim_macos(v0, tau, stop_threshold):
    ts, vs = [], []
    vel = v0
    acc = 0.0
    for t in np.arange(0, TOTAL_MS, TICK_MS):
        ts.append(t)
        vs.append(vel)
        vel *= np.exp(-TICK_MS / tau)
        if abs(vel) < stop_threshold:
            break
        acc += vel * (TICK_MS / 1000.0) * SCROLL_FACTOR
        emit = int(acc)
        acc -= emit
    return ts, vs


def main():
    parser = argparse.ArgumentParser(description="Plot rinertia damping curves")
    parser.add_argument("-o", "--output", default="damping_curves.png")
    parser.add_argument(
        "--v0", type=float, default=70000.0, help="Initial velocity (hires/sec)"
    )
    parser.add_argument(
        "--dual",
        action="append",
        metavar="DAMPING,THRESHOLD,LINEAR_MS[,TAIL_MS]",
        help="Add custom dual curve (can be repeated)",
    )
    parser.add_argument(
        "--no-defaults", action="store_true", help="Hide default curves"
    )
    args = parser.parse_args()

    v0 = args.v0
    curves = []

    if not args.no_defaults:
        t, v = sim_expo(v0, 0.05)
        curves.append(("expo (d=0.05)", t, v, "#e74c3c", "--", 2))

        t, v = sim_macos(v0, 325.0, 60.0)
        curves.append(("macos (tau=325)", t, v, "#2ecc71", "--", 2))

        t, v = sim_dual(v0, 0.05, 60, 384, 1, 0)
        curves.append(("dual default (th=60)", t, v, "#3498db", "-", 2.5))

    colors = ["#e67e22", "#9b59b6", "#e91e63", "#1abc9c", "#34495e", "#f39c12"]
    if args.dual:
        for i, spec in enumerate(args.dual):
            parts = spec.split(",")
            damping = float(parts[0])
            threshold = float(parts[1])
            linear_ms = float(parts[2])
            tail_ms = float(parts[3]) if len(parts) > 3 else 0
            label = f"dual d={damping} th={threshold:.0f} ld={linear_ms:.0f}"
            if tail_ms > 0:
                label += f" tail={tail_ms:.0f}"
            t, v = sim_dual(v0, damping, threshold, linear_ms, 1, tail_ms)
            color = colors[i % len(colors)]
            curves.append((label, t, v, color, "-", 2.5))

    fig, ax = plt.subplots(figsize=(14, 7))

    for label, ts, vs, color, ls, lw in curves:
        duration = ts[-1]
        ax.plot(
            ts,
            vs,
            label=f"{label} ({duration:.0f}ms)",
            linewidth=lw,
            color=color,
            linestyle=ls,
        )

    ax.set_xlabel("Time (ms) after finger lift", fontsize=13)
    ax.set_ylabel("Scroll velocity (hires/sec)", fontsize=13)
    ax.set_title(
        f"rinertia damping curves  (V0={v0:.0f}, 8ms tick)",
        fontsize=14,
        fontweight="bold",
    )
    ax.legend(fontsize=10, loc="upper right")
    ax.grid(True, alpha=0.3)
    ax.set_xlim(0, max(ts[-1] for _, ts, _, _, _, _ in curves) * 1.1)
    ax.set_ylim(0, v0 * 1.05)

    plt.tight_layout()
    plt.savefig(args.output, dpi=150)
    print(f"Saved: {args.output}")
    for label, ts, vs, *_ in curves:
        print(f"  {label}: {ts[-1]:.0f}ms")


if __name__ == "__main__":
    main()

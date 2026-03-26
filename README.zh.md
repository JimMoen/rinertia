# rinertia

Linux 笔记本触摸板的动量滚动。

Synaptics 触摸板驱动曾提供动量（惯性）滚动功能——两指快速滑动后页面会继续滑行。当 Linux 桌面迁移到 libinput 后，这一功能被移除了。rinertia 以独立的用户空间守护进程形式将其带回，全系统生效，兼容任何 Wayland 合成器、任何应用，开箱即用。

## 工作原理

```
内核 evdev ──► rinertia（被动读取）──► 动量引擎 ──► uinput 虚拟设备
                  │                                        │
                  │  不依赖 libinput，不 GRAB 设备          │
                  ▼                                        ▼
           触摸板事件照常流向                         注入 REL_WHEEL_HI_RES
           libinput / 合成器                         到输入栈
```

rinertia 通过 evdev 直接从 `/dev/input/eventN` 读取原始触摸板事件，**不依赖也不干扰 libinput**。由于从不抢占设备，你的合成器、libinput 以及手势工具（如 [fusuma](https://github.com/iberianpig/fusuma)）完全不受影响。

## 特性

- **即插即用** — 自动检测触摸板，直接运行
- **无侵入** — 被动读取事件，从不抢占触摸板
- **全局生效** — GTK、Qt、Electron、Firefox 等所有应用
- **可中断** — 触摸触摸板、按键或移动鼠标立即停止
- **可调优** — 阻尼、衰减曲线、速度均可调节

## 安装

```bash
cargo build --release
```

## 使用

```bash
# 开箱即用 — 自动检测触摸板，默认参数
sudo rinertia

# 按名称匹配触摸板
sudo rinertia -n "ELAN"

# 更长、更丝滑的惯性
sudo rinertia --damping 0.03 --linear-decel-ms 500

# 更短、更干脆的惯性
sudo rinertia --damping 0.10 --linear-decel-ms 200

# 纯指数衰减（无线性尾段）
sudo rinertia --damping-curve expo

# 排查问题 — 仅打印日志，不创建虚拟设备
sudo rinertia --dry --log-level debug

# 查看所有选项
rinertia --help
```

## 调优

| 参数 | 增大的效果 |
|------|-----------|
| `--damping` | 衰减更快，惯性更短 |
| `--tp-to-hires` | 每次手势滚动更远 |
| `--linear-decel-ms` | 尾段更慢、更长 |
| `--scroll-factor` | 每帧输出更多 |

## 已知问题

- **Chromium 系浏览器**自带 smooth scrolling，可能与 rinertia 叠加。可通过 `chrome://flags/#smooth-scrolling` 关闭，或用 `--scroll-factor` 补偿。
- `--tp-to-hires` 因设备而异 — 如果滚动过快或过慢，优先调这个参数。
- 指针惯性（`--mode pointer`）为实验性功能。

## 致谢

- [fusuma](https://github.com/iberianpig/fusuma) — 多点触控手势识别器，其架构启发了本项目的被动 evdev 监听设计
- [waynaptics](https://github.com/kekekeks/waynaptics) — Wayland synaptics 驱动适配层，本项目的双阶段动量引擎最初移植自该项目
- [xkeysnail](https://github.com/mooz/xkeysnail) — 基于 evdev 的按键重映射工具，其被动 evdev 监听方式为本项目的无 GRAB 设计提供了参考

## 许可

MIT

# rinertia

Linux 触摸板的 macOS 风格惯性滚动。

两指滚动后抬起手指，页面继续滑行 — 就像 Mac 一样。全系统生效，兼容任何 Wayland 合成器、任何应用，开箱即用。

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
sudo rinertia --decay-mode expo

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

## 许可

MIT

# sema

Arch Linux 系统资源监控守护工具。实时监控 CPU、内存、Swap、电池状态和时间，超出阈值时通过桌面通知（`notify-send`）发出提醒，每条通知有 1 分钟冷却防刷。

## 功能

| 指标 | 阈值 | 说明 |
|------|------|------|
| CPU | > 50% | 全局 CPU 使用率超出 50% 时提醒 |
| 内存 | > 50% | 物理内存使用率超出 50% 时提醒 |
| Swap | > 80% | Swap 使用率超出 80% 时提醒 |
| 电池 | < 84% | 电池剩余电量低于 84% 时提醒 |
| 时间 | :00 / :30 | 每小时整点和半点报时提醒 |

- 通知冷却：每个指标独立冷却，发送通知后 60 秒内不再重复
- 检测间隔：每 10 秒扫描一次

## 依赖

- **Rust** 1.85+
- **libnotify**（提供 `notify-send`，大多数桌面环境已预装）

## 安装

### 从源码编译

```bash
git clone https://github.com/codethare/sema.git
cd sema
cargo build --release
sudo cp target/release/sema /usr/local/bin/
```

### 从 Release 下载

从 [Releases](https://github.com/codethare/sema/releases) 下载预编译的二进制文件：

```bash
chmod +x sema
sudo mv sema /usr/local/bin/
```

## 使用

直接运行即可：

```bash
sema
```

建议添加到自动启动（如 i3/sway 的 `~/.config/sway/config` 或 KDE/GNOME 的自动启动设置）：

```bash
exec --no-startup-id sema
```

### Systemd 用户服务（可选）

创建 `~/.config/systemd/user/sema.service`：

```ini
[Unit]
Description=sema System Monitor
After=graphical-session.target

[Service]
ExecStart=/usr/local/bin/sema
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
```

启用：

```bash
systemctl --user enable --now sema
```

## 使用的 crate

| crate | 用途 | 版本 |
|-------|------|------|
| [sysinfo](https://crates.io/crates/sysinfo) | CPU / 内存 / Swap 信息 | 0.33 |
| [notify-rust](https://crates.io/crates/notify-rust) | 桌面通知（通过 notify-send） | 4.11 |
| [chrono](https://crates.io/crates/chrono) | 时间处理 | 0.4 |

电池信息直接读取 Linux 内核 sysfs (`/sys/class/power_supply/BAT*/capacity`)，零额外依赖。

## 许可证

MIT

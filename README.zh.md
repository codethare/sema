# sema (σῆμα)

Arch Linux 系统资源监控守护工具。实时监控 CPU、内存、Swap、电池、温度和网络流量，超出阈值时通过桌面通知（`notify-send`）发出提醒。

> **σῆμα** (sêma) — 古希腊语"信号、警报"。

## 功能

| 指标 | 默认阈值 | 说明 |
|------|----------|------|
| CPU | > 50% | 全局 CPU 使用率，通知中显示最耗 CPU 的进程 |
| 内存 | > 50% | 物理内存使用率，通知中显示最耗内存的进程 |
| Swap | > 80% | Swap 使用率 |
| 电池 | < 84% | 电池剩余电量 |
| 温度 | > 80°C | CPU/GPU/NVMe 组件温度，显示最热传感器 |
| 网络 | > 100 MB/s | 网络总吞吐量（RX + TX） |
| 时间 | :00 / :30 | 每小时整点和半点报时提醒 |

- 通知冷却：每个指标独立冷却，发送通知后冷却时间内不再重复
- 通知显示**持续时长**（如"持续 5m30s"）
- 通知显示**资源消耗最高的进程**（CPU/内存告警）
- 全面可配置：阈值、冷却时间、启用/禁用均可通过 TOML 调节
- 告警记录到日志文件，**超过 5MB 自动轮转**
- **SIGHUP 热重载** — 发送 HUP 信号即可重载配置，无需重启
- **sd_notify** 支持 — 与 systemd watchdog 集成
- 遵循 XDG 目录标准

## 安装

### 从源码编译

```bash
git clone https://github.com/codethare/sema.git
cd sema
cargo build --release
sudo cp target/release/sema /usr/local/bin/
```

### 从 Release 下载

从 [Releases](https://github.com/codethare/sema/releases) 下载预编译的归档包：

```bash
# 下载并校验
curl -OL https://github.com/codethare/sema/releases/latest/download/sema-x86_64-linux-musl.tar.gz
curl -OL https://github.com/codethare/sema/releases/latest/download/sema-x86_64-linux-musl.tar.gz.sha256

# 校验 SHA256
sha256sum --check sema-x86_64-linux-musl.tar.gz.sha256

# 解压安装
tar xzf sema-x86_64-linux-musl.tar.gz
sudo mv sema /usr/local/bin/
```

## 使用

直接运行进入守护模式：

```bash
sema
```

查看系统状态概览（不发送通知）：

```bash
sema --dry-run    # 或 sema -n
```

显示帮助信息：

```bash
sema --help
```

不重启重新加载配置（发送 SIGHUP）：

```bash
killall -HUP sema
```

开机自启（i3/sway 的 `~/.config/sway/config`）：

```bash
exec --no-startup-id sema
```

### Systemd 用户服务

sema 支持 `sd_notify` 协议，`systemctl status` 可查看实时状态：

```ini
# ~/.config/systemd/user/sema.service
[Unit]
Description=sema System Monitor
After=graphical-session.target

[Service]
Type=notify
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

使用 `Type=notify` 后，systemd 会等待 sema 发出就绪信号后才标记服务为 active，sema 会定期发送 `WATCHDOG=1`。

## 配置

配置文件位于 `~/.config/sema/config.toml`（或 `$XDG_CONFIG_HOME/sema/config.toml`）。

所有字段均有默认值，配置文件可以只包含你想修改的部分。

### 完整示例

```toml
# 全局检测间隔（秒），默认 10
check_interval_secs = 10

[cpu]
enabled = true
threshold = 50.0
cooldown_secs = 60

[memory]
enabled = true
threshold = 50.0
cooldown_secs = 60

[swap]
enabled = true
threshold = 80.0
cooldown_secs = 60

[battery]
enabled = true
threshold = 84.0
cooldown_secs = 60

[time]
enabled = true
cooldown_secs = 60

[temperature]
enabled = true
threshold = 80.0
cooldown_secs = 60

[network]
enabled = true
threshold = 100.0
cooldown_secs = 60
```

### 参数说明

| 参数 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `check_interval_secs` | 整数 | 10 | 全局检测间隔（秒） |
| `[metric].enabled` | 布尔 | true | 启用/禁用该指标 |
| `[metric].threshold` | 浮点数 | 见上表 | 告警阈值 |
| `[metric].cooldown_secs` | 整数 | 60 | 该指标通知冷却时间（秒） |

> `[metric]` 可以是 `cpu`、`memory`、`swap`、`battery`、`temperature`。
> `[time]` 配置没有 `threshold` 字段。
> `[network]` 的 `threshold` 单位是 **MB/s**（总吞吐量），非百分比。

### 禁用某个指标

```toml
[cpu]
enabled = false
```

## 日志

所有触发的告警会记录到日志文件：

```
~/.local/share/sema/sema.log
```

格式：

```
[2026-05-08 12:34:56] ⚠️ CPU overloaded | CPU usage: 95.0% (top: firefox 42.3%, threshold: 50.0%) (持续 5m30s)
```

日志文件**超过 5MB 自动轮转**，旧文件重命名为 `sema.log.1`。

查看最新日志：

```bash
tail -f ~/.local/share/sema/sema.log
```

## SIGHUP 热重载

sema 收到 `SIGHUP` 信号时，会重新加载配置文件并重建所有检测器，无需重启进程：

```bash
killall -HUP sema
```

适合在运行时调整阈值或开关指标。

## 使用的 crate

| crate | 用途 | 版本 |
|-------|------|------|
| [sysinfo](https://crates.io/crates/sysinfo) | CPU / 内存 / Swap / 进程 / 温度 | 0.33 |
| [notify-rust](https://crates.io/crates/notify-rust) | 桌面通知（通过 notify-send） | 4.11 |
| [chrono](https://crates.io/crates/chrono) | 时间处理 | 0.4 |
| [serde](https://crates.io/crates/serde) | 配置序列化 | 1 |
| [toml](https://crates.io/crates/toml) | TOML 配置解析 | 0.8 |
| [signal-hook](https://crates.io/crates/signal-hook) | SIGHUP 信号处理 | 0.3 |

电池信息直接读取 Linux 内核 sysfs (`/sys/class/power_supply/BAT*/capacity`)，零额外依赖。

## 许可证

MIT

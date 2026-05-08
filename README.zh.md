# sema (σῆμα)

Arch Linux 系统资源监控守护工具。实时监控 CPU、内存、Swap、电池状态和时间，超出阈值时通过桌面通知（`notify-send`）发出提醒。

> **σῆμα** (sêma) — 古希腊语"信号、警报"。

## 功能

| 指标 | 默认阈值 | 说明 |
|------|----------|------|
| CPU | > 50% | 全局 CPU 使用率超出阈值时提醒 |
| 内存 | > 50% | 物理内存使用率超出阈值时提醒 |
| Swap | > 80% | Swap 使用率超出阈值时提醒 |
| 电池 | < 84% | 电池剩余电量低于阈值时提醒 |
| 时间 | :00 / :30 | 每小时整点和半点报时提醒 |

- 通知冷却：每个指标独立冷却，发送通知后在冷却时间内不再重复
- 全面可配置：阈值、冷却时间、启用/禁用均可通过 TOML 配置文件调节

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

开机自启（i3/sway 的 `~/.config/sway/config`）：

```bash
exec --no-startup-id sema
```

### Systemd 用户服务

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
```

### 参数说明

| 参数 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `check_interval_secs` | 整数 | 10 | 全局检测间隔（秒） |
| `[metric].enabled` | 布尔 | true | 启用/禁用该指标 |
| `[metric].threshold` | 浮点数 | 见上表 | 告警阈值（%） |
| `[metric].cooldown_secs` | 整数 | 60 | 该指标通知冷却时间（秒） |

> `[metric]` 可以是 `cpu`、`memory`、`swap`、`battery`。
> `[time]` 配置没有 `threshold` 字段。

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
[2026-05-08 12:34:56] ⚠️ CPU 负载过高 | 当前 CPU 使用率: 95.0%（阈值: 50.0%）
```

查看最新日志：

```bash
tail -f ~/.local/share/sema/sema.log
```

## 使用的 crate

| crate | 用途 | 版本 |
|-------|------|------|
| [sysinfo](https://crates.io/crates/sysinfo) | CPU / 内存 / Swap 信息 | 0.33 |
| [notify-rust](https://crates.io/crates/notify-rust) | 桌面通知（通过 notify-send） | 4.11 |
| [chrono](https://crates.io/crates/chrono) | 时间处理 | 0.4 |
| [serde](https://crates.io/crates/serde) | 配置序列化 | 1 |
| [toml](https://crates.io/crates/toml) | TOML 配置解析 | 0.8 |

电池信息直接读取 Linux 内核 sysfs (`/sys/class/power_supply/BAT*/capacity`)，零额外依赖。

## 许可证

MIT

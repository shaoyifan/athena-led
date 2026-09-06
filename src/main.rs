mod led_screen;
mod char_dict;

use std::time::{Duration, Instant};
use anyhow::{Result, Context};
use chrono::Local;
use clap::Parser;
use std::collections::HashMap;
use std::cell::RefCell;
use std::env;
use std::fs;
use reqwest::Client;
use tokio::time;  // 用于 time::sleep()

// ==========================================
// 网速缓存 (用于计算实时网速)
// ==========================================
// Key: 网卡名, Value: (上次RX字节数, 上次TX字节数, 上次记录时间)
thread_local! {
    static NET_SPEED_CACHE: RefCell<HashMap<String, (u64, u64, Instant)>> = RefCell::new(HashMap::new());
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, default_value = "")]
    status: String,

    #[arg(long, default_value_t = 5)]
    seconds: u64,

    #[arg(long, default_value_t = 5)]
    light_level: u8,

    #[arg(long, default_value = "date timeBlink")]
    option: String,

    #[arg(long, default_value = "abcdefghijklmnopqrstuvwxyz0123456789+-*/=.:：℃")]
    value: String,

    #[arg(long, default_value = "https://www.baidu.com/")]
    url: String,

    #[arg(long, default_value = "4")]
    temp_flag: String,

    /// 网卡名称，用于读取实时网速 (如: wan, eth0, br-lan)
    #[arg(long, default_value = "wan")]
    net_interface: String,
}

fn set_timezone_from_config() -> Result<()> {
    let content = fs::read_to_string("/etc/config/system")
        .context("Failed to read system config")?;

    for line in content.lines() {
        let line = line.trim();

        if line.starts_with("option timezone") {
            if let Some(tz) = line.split('\'').nth(1) {
                if tz == "CST-8" {
                    env::set_var("TZ", "Asia/Shanghai");
                    return Ok(());
                }
            }
        } else if line.starts_with("option zonename") {
            if let Some(zone) = line.split('\'').nth(1) {
                env::set_var("TZ", zone);
                return Ok(());
            }
        }
    }

    env::set_var("TZ", "UTC");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    set_timezone_from_config()?;

    let args = Args::parse();

    let client = Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .context("Failed to create HTTP client")?;

    let mut screen = led_screen::LedScreen::new(581, 582, 585, 586)
        .context("Failed to initialize LED screen")?;

    screen.power(true, args.light_level)
        .context("Failed to power on LED screen")?;

    let status_flag = args.status.split_whitespace()
        .fold(0u8, |acc, item| {
            acc | match item {
                "clock" => 1,
                "medal" => 2,
                "upload" => 4,
                "download" => 8,
                _ => 0,
            }
        });

    // 使用 tokio 信号处理 (如果 Unix 平台)
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = signal(SignalKind::terminate())?;
        let mut sigint = signal(SignalKind::interrupt())?;
        let mut sighup = signal(SignalKind::hangup())?;

        loop {
            tokio::select! {
                _ = sigterm.recv() => {
                    screen.power(false, 0)?;
                    break;
                },

                _ = sigint.recv() => {
                    screen.power(false, 0)?;
                    break;
                },

                _ = sighup.recv() => {
                    screen.power(false, 0)?;
                    break;
                },

                _ = process_options(&mut screen, &args, status_flag, &client) => {},
            }
        }
    }

    #[cfg(not(unix))]
    {
        // 非 Unix 平台直接循环处理
        process_options(&mut screen, &args, status_flag, &client).await?;
    }

    Ok(())
}

// ==========================================
// 网速获取函数 (基于 /proc/net/dev)
// ==========================================

/// 格式化网速为可读字符串
fn format_bytes_speed(bytes_per_sec: f64) -> String {
    if bytes_per_sec > 1_048_576.0 {
        format!("{:.1}M", bytes_per_sec / 1_048_576.0)
    } else if bytes_per_sec > 1024.0 {
        format!("{:.0}K", bytes_per_sec / 1024.0)
    } else {
        format!("{:.1}K", bytes_per_sec / 1024.0)
    }
}

/// 读取指定网卡的字节数 (rx_bytes, tx_bytes)
fn read_net_bytes_for(target_iface: &str) -> (u64, u64) {
    if let Ok(content) = fs::read_to_string("/proc/net/dev") {
        for line in content.lines() {
            // 精确匹配网卡名：去除前导空格后，行以 "接口名:" 开头
            let trimmed = line.trim_start();
            if trimmed.starts_with(&format!("{}:", target_iface)) {
                if let Some((_, data)) = line.split_once(':') {
                    let parts: Vec<&str> = data.split_whitespace().collect();
                    if parts.len() >= 9 {
                        let rx = parts[0].parse::<u64>().unwrap_or(0);  // 接收字节数
                        let tx = parts[8].parse::<u64>().unwrap_or(0);  // 发送字节数
                        return (rx, tx);
                    }
                }
            }
        }
    }
    (0, 0)
}

/// 获取指定网卡的实时网速字符串
/// mode: 0 = 下载 (rx), 1 = 上传 (tx)
fn get_speed_string(mode: u8, target_iface: &str) -> String {
    let (curr_rx, curr_tx) = read_net_bytes_for(target_iface);
    let now = Instant::now();

    NET_SPEED_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();

        // 先取出旧值（克隆，避免借用冲突）
        let (last_rx, last_tx, last_time) = cache.get(target_iface)
            .map(|(rx, tx, t)| (*rx, *tx, *t))
            .unwrap_or((0, 0, now));

        let duration = now.duration_since(last_time).as_secs_f64();

        // 防抖与异常防护：首次调用或时间间隔异常
        if duration < 0.1 || duration > 30.0 || last_rx == 0 {
            cache.insert(target_iface.to_string(), (curr_rx, curr_tx, now));
            return format_bytes_speed(0.0);
        }

        // 计算网速
        let speed = if mode == 0 {
            (curr_rx.saturating_sub(last_rx)) as f64 / duration  // 下载
        } else {
            (curr_tx.saturating_sub(last_tx)) as f64 / duration  // 上传
        };

        // 更新缓存
        cache.insert(target_iface.to_string(), (curr_rx, curr_tx, now));

        format_bytes_speed(speed)
    })
}

/// 读取 netfilter conntrack 表的当前活动连接数
/// 数据源: /proc/sys/net/netfilter/nf_conntrack_count (与 OpenWrt 首页状态一致)
/// 注意: 需要内核加载 nf_conntrack 模块；模块未加载时返回 0
fn get_connection_count() -> u32 {
    fs::read_to_string("/proc/sys/net/netfilter/nf_conntrack_count")
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(0)
}

/// 读取 NSS 固件整体 CPU 负载 (如 "99%")
/// 数据源: /sys/kernel/debug/qca-nss-drv/stats/cpu_load_ubi 第 6 行第 2 列
/// 等价于 shell: awk 'NR == 6 { print $2; exit }' /sys/kernel/debug/qca-nss-drv/stats/cpu_load_ubi
/// 注意: 需要内核加载 qca-nss-drv 且 debugfs 已挂载；读不到时返回 None
fn get_nss_load() -> Option<String> {
    fs::read_to_string("/sys/kernel/debug/qca-nss-drv/stats/cpu_load_ubi")
        .ok()
        .and_then(|s| {
            s.lines()
                .nth(5)                              // NR == 6 (0 起始)
                .and_then(|l| l.split_whitespace().nth(1))  // $2
                .map(|v| v.to_string())
        })
}

async fn process_options(
    screen: &mut led_screen::LedScreen,
    args: &Args,
    status: u8,
    client: &Client,
) -> Result<()> {

    for option in args.option.split_whitespace() {
        match option {

            "date" => {
                let time = Local::now().format("%m-%d").to_string();

                screen.write_data(&time, status)?;

                time::sleep(Duration::from_secs(args.seconds)).await;
            }

            "time" => {
                let time = Local::now().format("%H:%M").to_string();

                screen.write_data(&time, status)?;

                time::sleep(Duration::from_secs(args.seconds)).await;
            }

            "timeBlink" => {
                let start = Instant::now();

                let mut time_flag = false;

                while start.elapsed() < Duration::from_secs(args.seconds) {

                    let mut time = Local::now()
                        .format("%H:%M")
                        .to_string();

                    if time_flag {
                        time = time.replace(':', "  ");
                    }

                    screen.write_data(&time, status)?;

                    time_flag = !time_flag;

                    time::sleep(Duration::from_secs(1)).await;
                }
            }

            "temp" => {
                if let Some(temp) = get_temp(&args.temp_flag)? {

                    screen.write_data(&temp, status)?;

                    time::sleep(Duration::from_secs(args.seconds)).await;
                }
            }

            "upload" => {
                // 实时显示上传网速（每个 IntervalTime 周期内每 1 秒刷新一次）
                let start = Instant::now();
                while start.elapsed() < Duration::from_secs(args.seconds) {
                    let speed = get_speed_string(1, &args.net_interface);
                    // 保留用户配置的侧边指示灯，并点亮上传指示灯 (bit 4)
                    screen.write_data(&speed, status | 4)?;
                    time::sleep(Duration::from_secs(1)).await;
                }
            }

            "download" => {
                // 实时显示下载网速（每个 IntervalTime 周期内每 1 秒刷新一次）
                let start = Instant::now();
                while start.elapsed() < Duration::from_secs(args.seconds) {
                    let speed = get_speed_string(0, &args.net_interface);
                    // 保留用户配置的侧边指示灯，并点亮下载指示灯 (bit 8)
                    screen.write_data(&speed, status | 8)?;
                    time::sleep(Duration::from_secs(1)).await;
                }
            }

            "connection" => {
                // 实时显示当前 TCP 总连接数（每 1 秒刷新一次）
                let start = Instant::now();
                while start.elapsed() < Duration::from_secs(args.seconds) {
                    let count = get_connection_count();
                    let text = format!("# {}", count);  // 显示为 #123 (# 不受 to_uppercase 影响)
                    screen.write_data(&text, status)?;
                    time::sleep(Duration::from_secs(1)).await;
                }
            }

            "nss" => {
                // 实时显示 NSS 负载 NS:99% （每 1 秒刷新一次）
                let start = Instant::now();
                while start.elapsed() < Duration::from_secs(args.seconds) {
                    let text = match get_nss_load() {
                        Some(v) => format!("NS:{}", v),
                        None => "NS:--".to_string(),  // qca-nss-drv 未加载 / debugfs 未挂载
                    };
                    screen.write_data(&text, status)?;
                    time::sleep(Duration::from_secs(1)).await;
                }
            }

            "string" => {
                screen.write_data(&args.value, status)?;
                time::sleep(Duration::from_secs(args.seconds)).await;
            }

            "getByUrl" => {

                if let Ok(resp) = client.get(&args.url).send().await {

                    if let Ok(text) = resp.text().await {

                        screen.write_data(&text, status)?;

                        time::sleep(Duration::from_secs(args.seconds)).await;
                    }
                }
            }

            _ => {}
        }
    }

    Ok(())
}

fn get_temp(temp_flags: &str) -> Result<Option<String>> {

    let mut result = String::new();

    // 解析 UCI MultiValue: 空格分隔的 token
    //  0~6 → /sys/class/thermal/thermal_zone{N}/temp
    //  7   → WiFi 5.8G  (ieee80211 phy0)   ← ipq60xx 三频机型
    //  8   → WiFi 2.4G  (ieee80211 phy1)
    //  9   → WiFi 5.2G  (ieee80211 phy2)
    //  ipq60xx / ipq8074 / mt76 等 SoC 的常见布局都能匹配
    for token in temp_flags.split_whitespace() {
        let temp_celsius = match token {
            "7" => read_wifi_temp(0),
            "8" => read_wifi_temp(1),
            "9" => read_wifi_temp(2),
            n => {
                if let Ok(idx) = n.parse::<u32>() {
                    if idx <= 6 {
                        let path =
                            format!("/sys/class/thermal/thermal_zone{}/temp", idx);
                        read_temp_celsius(&path)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        };

        if let Some(c) = temp_celsius {

            if !result.is_empty() {
                result.push(' ');
            }

            result.push_str(
                &format!("{:.1}℃", c)
            );
        }
    }

    Ok(
        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    )
}

/// 读取 `/sys/.../temp*_input` (毫摄氏度) 并换算为摄氏度
fn read_temp_celsius(path: &str) -> Option<f64> {
    let s = fs::read_to_string(path).ok()?;
    let raw = s.trim().parse::<f64>().ok()?;
    Some(raw / 1000.0)
}

/// 读取指定 phy 的 WiFi 芯片温度
/// 取 /sys/class/ieee80211/phy{N}/ 下第一个 hwmon* 子目录里的 temp1_input
fn read_wifi_temp(phy_idx: usize) -> Option<f64> {
    let phy_dir = format!("/sys/class/ieee80211/phy{}", phy_idx);
    let entries = fs::read_dir(&phy_dir).ok()?;

    for e in entries.flatten() {
        if e.file_name().to_string_lossy().starts_with("hwmon") && e.path().is_dir() {
            return read_temp_celsius(
                &e.path().join("temp1_input").to_string_lossy()
            );
        }
    }
    None
}
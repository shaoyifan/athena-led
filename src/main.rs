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
        format!("{:.0}B", bytes_per_sec)
    }
}

/// 读取指定网卡的字节数 (rx_bytes, tx_bytes)
fn read_net_bytes_for(target_iface: &str) -> (u64, u64) {
    if let Ok(content) = fs::read_to_string("/proc/net/dev") {
        for line in content.lines() {
            if line.contains(target_iface) {
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

        // 获取或初始化该网卡的缓存数据
        let (last_rx, last_tx, last_time) = cache
            .entry(target_iface.to_string())
            .or_insert((curr_rx, curr_tx, now));

        let duration = now.duration_since(*last_time).as_secs_f64();

        // 防抖与异常防护
        if duration < 0.1 || duration > 30.0 || *last_rx == 0 {
            cache.insert(target_iface.to_string(), (curr_rx, curr_tx, now));
            return format_bytes_speed(0.0);
        }

        // 计算网速
        let speed = if mode == 0 {
            (curr_rx.saturating_sub(*last_rx)) as f64 / duration  // 下载
        } else {
            (curr_tx.saturating_sub(*last_tx)) as f64 / duration  // 上传
        };

        // 更新缓存
        cache.insert(target_iface.to_string(), (curr_rx, curr_tx, now));

        format_bytes_speed(speed)
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
                let start = time::Instant::now();

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

            "string" => {

                if args.value == "ud" {
                    // 每个周期：下载、上传各显示一半时间
                    let half = args.seconds / 2;
                    let start = time::Instant::now();

                    while start.elapsed() < Duration::from_secs(args.seconds) {
                        // 显示下载
                        let speed = get_speed_string(0, &args.net_interface);
                        let _ = screen.write_data(&speed, 8);
                        time::sleep(Duration::from_secs(half)).await;

                        // 显示上传
                        let speed = get_speed_string(1, &args.net_interface);
                        let _ = screen.write_data(&speed, 4);
                        time::sleep(Duration::from_secs(half)).await;
                    }

                } else if args.value == "d" {
                    // 只显示下载
                    let speed = get_speed_string(0, &args.net_interface);
                    screen.write_data(&speed, 8)?;
                    time::sleep(Duration::from_secs(args.seconds)).await;

                } else if args.value == "u" {
                    // 只显示上传
                    let speed = get_speed_string(1, &args.net_interface);
                    screen.write_data(&speed, 4)?;
                    time::sleep(Duration::from_secs(args.seconds)).await;

                } else {

                    screen.write_data(&args.value, status)?;

                    time::sleep(Duration::from_secs(args.seconds)).await;
                }
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

    for i in 0..=6 {

        if !temp_flags.contains(&i.to_string()) {
            continue;
        }

        let temp_path =
            format!("/sys/class/thermal/thermal_zone{}/temp", i);

        if let Ok(temp_str) = std::fs::read_to_string(&temp_path) {

            if let Ok(temp) = temp_str.trim().parse::<f64>() {

                let temp_celsius = temp / 1000.0;

                // 只有真正追加内容时才加空格
                if !result.is_empty() {
                    result.push(' ');
                }

                result.push_str(
                    &format!("{:.1}℃", temp_celsius)
                );
            }
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
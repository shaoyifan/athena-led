mod led_screen;
mod char_dict;

use std::time::Duration;
use anyhow::{Result, Context};
use chrono::Local;
use clap::Parser;
use tokio::signal::unix::{signal, SignalKind};
use tokio::time;
use std::env;
use std::fs;
use serde_json::Value;

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
}

fn set_timezone_from_config() -> Result<()> {
    // 读取 OpenWrt 系统配置文件
    let content = fs::read_to_string("/etc/config/system")
        .context("Failed to read system config")?;

    // 解析配置文件找到时区设置
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("option timezone") {
            if let Some(tz) = line.split('\'').nth(1) {
                // OpenWrt 使用 CST-8 格式表示东八区
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
    
    // 如果没有找到时区设置，使用默认值
    env::set_var("TZ", "UTC");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // 设置时区
    set_timezone_from_config()?;
    
    let args = Args::parse();
    
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
            _ = process_options(&mut screen, &args, status_flag) => {},
        }
    }
    
    Ok(())
}

/// 获取并格式化 Netdata 的流量数据
async fn fetch_netdata_traffic() -> Option<String> {
    let url = "http://10.0.0.1:19999/api/v1/allmetrics?format=json&filter=net.wan";
    
    // 1. 发起请求
    let resp = reqwest::get(url).await.ok()?;
    let json: Value = resp.json().await.ok()?;

    // 2. 提取数值: net.wan -> dimensions -> received -> value
    let raw_value = json["net.wan"]["dimensions"]["received"]["value"].as_f64()?;

    // 3. 单位转换
    // Netdata 默认单位通常是 kilobits/s
    // 转换为 KB/s (1 byte = 8 bits)
    let kb_s = raw_value / 8.0;

    // 4. 格式化输出
    if kb_s >= 1000.0 {
        // 超过 1000 KB/s 则转为 MB/s，保留一位小数
        Some(format!("{:.1}MB/s", kb_s / 1024.0))
    } else {
        // 保留整数
        Some(format!("{:.0}KB/s", kb_s))
    }
}

async fn process_options(screen: &mut led_screen::LedScreen, args: &Args, status: u8) -> Result<()> {
    for option in args.option.split_whitespace() {
        match option {
            "date" => {
                let time = Local::now().format("%m-%d").to_string();
                let spaced_time = time.chars().map(|c| c.to_string()).collect::<Vec<_>>().join(" ");
                screen.write_data(spaced_time.as_bytes(), status)?;
                time::sleep(Duration::from_secs(args.seconds)).await;
            }
            "time" => {
                let time = Local::now().format("%H:%M").to_string();
                let spaced_time = time.chars().map(|c| c.to_string()).collect::<Vec<_>>().join(" ");
                screen.write_data(spaced_time.as_bytes(), status)?;
                time::sleep(Duration::from_secs(args.seconds)).await;
            }
            "timeBlink" => {
                let start = time::Instant::now();
                let mut time_flag = false;
                while start.elapsed() < Duration::from_secs(args.seconds) {
                    let time = Local::now().format("%H:%M").to_string();
                    if time_flag {
                        spaced_time = spaced_time.replace(':', " ");
                    }
                    screen.write_data(spaced_time.as_bytes(), status)?;
                    time_flag = !time_flag;
                    time::sleep(Duration::from_secs(1)).await;
                }
            }
            "temp" => {
                if let Some(temp) = get_temp(&args.temp_flag)? {
                    screen.write_data(temp.as_bytes(), status)?;
                    time::sleep(Duration::from_secs(args.seconds)).await;
                }
            }
            "string" => {
                if args.value == "netdata" {
                    if let Some(display_text) = fetch_netdata_traffic().await {
                        screen.write_data(display_text.as_bytes(), status)?;
                    }
                } else {
                    screen.write_data(args.value.as_bytes(), status)?;
                }
                time::sleep(Duration::from_secs(args.seconds)).await;
            }
            "getByUrl" => {
                if let Ok(resp) = reqwest::get(&args.url).await {
                    if let Ok(text) = resp.text().await {
                        screen.write_data(text.as_bytes(), status)?;
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
        if i != 0 {
            result.push_str(" ");
        }

        if !temp_flags.contains(&i.to_string()) {
            continue;
        }
        
        let temp_path = format!("/sys/class/thermal/thermal_zone{}/temp", i);
        
        if let Ok(temp_str) = std::fs::read_to_string(&temp_path) {
            if let Ok(temp) = temp_str.trim().parse::<f64>() {
                // 核心修改：保留 1 位小数 + 加上 ℃
                let temp_celsius = temp / 1000.0;
                result.push_str(&format!("{:.1}℃", temp_celsius));
            }
        }
    }
    
    Ok(if result.is_empty() { None } else { Some(result) })
}
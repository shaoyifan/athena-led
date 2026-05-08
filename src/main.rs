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
                    let mut spaced_time = time.chars().map(|c| c.to_string()).collect::<Vec<_>>().join(" ");
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
                screen.write_data(args.value.as_bytes(), status)?;
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
        
        let type_path = format!("/sys/class/thermal/thermal_zone{}/type", i);
        let temp_path = format!("/sys/class/thermal/thermal_zone{}/temp", i);
        
        if let Ok(zone_type) = std::fs::read_to_string(&type_path) {
            if let Ok(temp_str) = std::fs::read_to_string(&temp_path) {
                if let Ok(temp) = temp_str.trim().parse::<f64>() {
                    let zone_type = zone_type.trim().replace("-thermal", "");
                    let spaced_zone_type = zone_type.chars().map(|c| c.to_string()).collect::<Vec<_>>().join(" ");
                    let temp_char= (temp / 1000.0).round().to_string();
                    let spaced_temp = temp_char.chars().map(|c| c.to_string()).collect::<Vec<_>>().join(" ");
                    result.push_str(&format!("{} : {}", spaced_zone_type, spaced_temp));
                }
            }
        }
    }
    
    Ok(if result.is_empty() { None } else { Some(result) })
}

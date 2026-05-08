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
use reqwest::Client;

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

    Ok(())
}

/// 获取并格式化 Netdata 流量数据
async fn fetch_netdata_traffic(client: &Client) -> Option<String> {
    let url = "http://10.0.0.1:19999/api/v1/allmetrics?format=json&filter=net.wan";

    let resp = client.get(url).send().await.ok()?;

    let json: Value = resp.json().await.ok()?;

    let raw_value = json["net.wan"]["dimensions"]["received"]["value"]
        .as_f64()?;

    // Netdata 默认通常为 kilobits/s
    let kb_s = raw_value / 8.0;

    if kb_s >= 1000.0 {
        Some(format!("↘{:.1}M", kb_s / 1024.0))
    } else {
        Some(format!("↘{:.0}K", kb_s))
    }
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

                if args.value == "netdata" {

                    if let Some(display_text) =
                        fetch_netdata_traffic(client).await {

                        screen.write_data(&display_text, status)?;

                        time::sleep(Duration::from_secs(args.seconds)).await;
                    }

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
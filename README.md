# Athena LED 控制器

[简体中文](README_zh.md) | [English](README.md)

[原项目](https://github.com/haipengno1/athena-led) 的分支，用于在 OpenWrt 设备上控制 LED 点阵显示屏。

## 功能特性

- 显示当前时间和日期
- 显示系统温度
- 自定义文本显示
- 可调节亮度级别
- 多种显示模式
- HTTP 状态监控

## 构建说明

1. 安装 Docker

   ```bash
   export DOWNLOAD_URL="https://mirrors.tuna.tsinghua.edu.cn/docker-ce"
   curl -fsSL https://raw.githubusercontent.com/docker/docker-install/master/install.sh | sh
   sudo usermod -aG docker $USER

   # 验证 Docker 安装
   docker --version
   ```

2. 安装与配置 Rustup

   ```bash
   sudo apt update
   sudo apt install rustup
   rustup toolchain install stable
   rustup default stable

   # 验证 rust 和 cargo
   rustc --version
   cargo --version

   # 配置 Rustup 目标
   rustup target add aarch64-unknown-linux-musl
   rustup target add x86_64-unknown-linux-musl
   ```

3. 安装 Cross

   ```bash
   cargo install cross --git https://github.com/cross-rs/cross
   ```

   将 Cross 添加到 PATH 中：

   ```bash
   # BASH SHELL
   echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc && source ~/.bashrc
   ```

   ```bash
   # ZSH SHELL
   echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc && source ~/.zshrc
   ```

   ```bash
   # FISH SHELL
   fish_add_path ~/.cargo/bin && source ~/.config/fish/config.fish
   ```

4. 构建项目：

   1. 使用 Cross 直接构建：

      ```bash
      # 构建 aarch64-unknown-linux-musl 目标
      cross build --release --target aarch64-unknown-linux-musl
      # 构建 x86_64-unknown-linux-musl 目标
      cross build --release --target x86_64-unknown-linux-musl
      ```

   2. 使用 Makefile 构建：

      ```bash
      # 构建所有目标
      make all
      # 构建特定目标
      make arm # 构建 aarch64-unknown-linux-musl
      make x64 # 构建 x86_64-unknown-linux-musl
      ```

      Makefile方式编译后的二进制文件位于 `dist/` 目录下

## 安装说明

将编译好的二进制文件重命名为 `athena-led` 复制到 OpenWrt 设备的 `/usr/sbin/` 目录下。

## 使用方法

```bash
athena-led [选项]

选项说明：
    --status <状态>            设置状态字符串 [默认: ""]
    --seconds <秒数>           更新间隔（秒） [默认: 5]
    --light-level <亮度>       设置亮度级别（0-7） [默认: 5]
    --option <选项>            显示模式（如 "date"、"timeBlink"） [默认: "date timeBlink"]
    --value <值>              自定义显示字符 [默认: "abcdefghijklmnopqrstuvwxyz0123456789+-*/=.:：℃"]
    --url <URL>               状态监控的 URL [默认: "https://www.baidu.com/"]
    --temp-flag <标志>         温度传感器标志（0:nss-top, 1:nss, 2:wcss-phya0, 3:wcss-phya1, 4:cpu, 5:lpass, 6:ddrss） [默认: "4"]
```

## 常见问题
1. **时间显示问题**  
   如果显示的时间与系统时区不匹配，请确保系统已安装所需的时区数据包：
   - OpenWrt 系统：安装 `zoneinfo-core` 和对应地区的包（如 `zoneinfo-asia`）
   - 其他 Linux 发行版：安装 `tzdata` 包

## 开源许可

本项目采用 Apache License 2.0 许可证 - 详见 [LICENSE](LICENSE) 文件。

1. 设置环境变量
GOARCH=arm64;GOOS=linux
2. 编译
go build -ldflags="-s -w" -trimpath -o athena-led main.go
3. 打包
复制`athena-led` 到`luci-app-athena-led/root/usr/sbin/`

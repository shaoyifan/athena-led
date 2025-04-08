package main

import (
	athenaLed "athenaLed/internal"
	"bufio"
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"math"
	"net/http"
	"os"
	"os/signal"
	"strconv"
	"strings"
	"syscall"
	"time"
	_ "time/tzdata"
)

var status byte = 0b00001111

func main() {
	screen, err := athenaLed.Init()
	if err != nil {
		fmt.Printf("Init error: %v\n", err)
		return
	}
	defer func() {
		err := screen.Destroy()
		if err != nil {
			fmt.Printf("DestroyExport error: %v\n", err)
		}
	}()
	go mainLoop(screen)
	<-getExitSign()
}

func mainLoop(screen athenaLed.LedScreen) {
	statusVar := flag.String("status", "", "led status")
	seconds := flag.Int("seconds", 5, "led switching time (second)")
	lightLevel := flag.Int("lightLevel", 5, "led light level 0-7")
	options := flag.String("option", "date timeBlink", "led option")
	value := flag.String("value", "abcdefghijklmnopqrstuvwxyz0123456789+-*/=.:：℃", "led content")
	url := flag.String("url", "https://www.baidu.com/", "api url for get content")
	tempFlag := flag.String("tempFlag", "4", "show temp for something,0-6")
	flag.Parse()

	var statusFlag byte = 0
	for _, item := range strings.Split(*statusVar, " ") {
		switch item {
		case "time":
			statusFlag |= 1
		case "medal":
			statusFlag |= 2
		case "upload":
			statusFlag |= 4
		case "download":
			statusFlag |= 8
		}
	}

	status = statusFlag << 4 >> 4

	fmt.Println(*statusVar, *seconds, *lightLevel, *options, *value, *url)
	err := screen.Power(true, byte(*lightLevel))
	if err != nil {
		fmt.Printf("SetPower error: %v\n", err)
		return
	}
	zoneName := getZoneName()
	timeFlag := false
	for {
	optionLoop:
		for _, option := range strings.Split(*options, " ") {
			fmt.Println(option)
			switch option {
			case "date":
				formattedTime := timeFormat(zoneName, "01-02")
				screen.WriteData(formattedTime, status)
				time.Sleep(time.Duration(*seconds) * time.Second)
			case "time":
				formattedTime := timeFormat(zoneName, "15:04")
				screen.WriteData(formattedTime, status)
				time.Sleep(time.Duration(*seconds) * time.Second)
			case "timeBlink":
				ctx, cancel := context.WithTimeout(context.Background(), time.Duration(*seconds)*time.Second)
				for {
					select {
					case <-ctx.Done():
						cancel()
						continue optionLoop
					default:
						formattedTime := timeFormat(zoneName, "15:04")
						if timeFlag {
							formattedTime = strings.ReplaceAll(formattedTime, ":", "  ")
						}
						timeFlag = !timeFlag
						screen.WriteData(formattedTime, status)
					}
					time.Sleep(1 * time.Second)
				}
			case "temp":
				tempString := getTemp(*tempFlag)
				if strings.EqualFold(tempString, "") {
					continue
				}
				screen.WriteData(tempString, status)
				time.Sleep(time.Duration(*seconds) * time.Second)
			case "string":
				screen.WriteData(*value, status)
				time.Sleep(time.Duration(*seconds) * time.Second)
			case "getByUrl":
				content, err := getByUrl(*url)
				if err != nil {
					fmt.Println("Error:", err)
					continue optionLoop
				}
				screen.WriteData(string(content), status)
				time.Sleep(time.Duration(*seconds) * time.Second)
			}
		}
	}
}

func getByUrl(url string) (string, error) {
	// 如果 URL 是 netdata，则请求特定的 API
	if strings.Contains(url, "netdata") {
		// 请求 netdata 的 API
		resp, err := http.Get("http://10.0.0.1:19999/api/v1/allmetrics?format=json&filter=net.wan")
		if err != nil {
			return "", fmt.Errorf("error fetching netdata URL: %v", err)
		}
		defer resp.Body.Close()

		// 读取响应体
		body, err := io.ReadAll(resp.Body)
		if err != nil {
			return "", fmt.Errorf("error reading netdata response body: %v", err)
		}

		// 解析 JSON 数据
		type Dimension struct {
			Value float64 `json:"value"`
		}

		type Dimensions struct {
			Received Dimension `json:"received"`
			Sent     Dimension `json:"sent"`
		}

		type NetWan struct {
			Dimensions Dimensions `json:"dimensions"`
		}

		type NetdataResponse struct {
			NetWan NetWan `json:"net.wan"`
		}

		var netdataResponse NetdataResponse
		err = json.Unmarshal(body, &netdataResponse)
		if err != nil {
			return "", fmt.Errorf("error parsing netdata JSON: %v", err)
		}

		// 随机选择 send 或 received
		// rand.Seed(time.Now().UnixNano())
		var selectedValue float64
		var prefix string
		// if rand.Intn(2) == 0 {
		selectedValue = netdataResponse.NetWan.Dimensions.Received.Value
		prefix = "↘"
		// } else {
		// 	selectedValue = netdataResponse.NetWan.Dimensions.Sent.Value
		// 	prefix = "↗"
		// }

		// 将值从 kilobits/s 转换为 KB/s
		selectedValueKBps := math.Abs(selectedValue / 8.0)

		// 判断是否超过 1000 KB/s
		if selectedValueKBps > 999 {
			// 转换为 MB/s
			selectedValueMBps := selectedValueKBps / 1024.0
			return fmt.Sprintf("%s%.1fM", prefix, selectedValueMBps), nil
		}
		// 返回值为 KB/s
		return fmt.Sprintf("%s%dK", prefix, int(selectedValueKBps)), nil
	}

	// 默认处理其他 URL
	resp, err := http.Get(url)
	if err != nil {
		return "", fmt.Errorf("error fetching URL: %v", err)
	}
	defer resp.Body.Close()

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return "", fmt.Errorf("error reading response body: %v", err)
	}

	return string(body), nil
}

func getTemp(tempFlags string) string {
	value := ""
	for i := 0; i <= 6; i++ {
		if !strings.Contains(tempFlags, strconv.Itoa(i)) {
			continue
		}

		typePath := fmt.Sprintf("/sys/class/thermal/thermal_zone%d/type", i)
		tempPath := fmt.Sprintf("/sys/class/thermal/thermal_zone%d/temp", i)

		zoneType, err := os.ReadFile(typePath)
		if err != nil {
			fmt.Printf("getTemp type from %s error: %v\n", typePath, err)
			continue
		}
		tempData, err := os.ReadFile(tempPath)
		if err != nil {
			fmt.Printf("getTemp value from %s error: %v\n", tempPath, err)
			continue
		}

		tempStr := strings.TrimSpace(string(tempData))
		tempInt, err := strconv.Atoi(tempStr)
		if err != nil {
			fmt.Printf("getTemp strconv.Atoi error: %v\n", tempStr)
			continue
		}
		value += fmt.Sprintf("%s:%.1f℃   ", strings.ReplaceAll(strings.TrimSpace(string(zoneType)), "-thermal", ""), float64(tempInt)/1000.0)
	}
	return value
}

func timeFormat(zoneName, layout string) string {
	loc, _ := time.LoadLocation(zoneName)
	currentTime := time.Now().In(loc)
	formattedTime := currentTime.Format(layout)
	return formattedTime
}

func getZoneName() string {
	zoneName := "Asia/Shanghai"
	file, err := os.Open("/etc/config/system")
	if err != nil {
		fmt.Println("Error opening file:", err)
		return zoneName
	}
	defer func(file *os.File) {
		_ = file.Close()
	}(file)

	scanner := bufio.NewScanner(file)
	for scanner.Scan() {
		line := scanner.Text()
		if strings.Contains(line, "option zonename") {
			zoneName = strings.TrimSpace(strings.Split(line, "'")[1])
			if strings.Contains(zoneName, " ") {
				zoneName = strings.ReplaceAll(zoneName, " ", "_")
			}
			continue
		}
	}
	return zoneName
}

func getExitSign() <-chan os.Signal {
	quit := make(chan os.Signal, 1)
	signal.Notify(quit,
		syscall.SIGHUP,
		syscall.SIGINT,
		syscall.SIGQUIT,
		syscall.SIGILL,
		syscall.SIGTRAP,
		syscall.SIGABRT,
		syscall.SIGBUS,
		syscall.SIGFPE,
		syscall.SIGKILL,
		syscall.SIGSEGV,
		syscall.SIGPIPE,
		syscall.SIGALRM,
		syscall.SIGTERM)
	return quit
}

package main

import (
	athenaLed "athenaLed/internal"
	"bufio"
	"context"
	"flag"
	"fmt"
	"io"
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
	timeSwitch := flag.Int("seconds", 5, "led switching time (second)")
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

	fmt.Println(*statusVar, *timeSwitch, *lightLevel, *options, *value, *url)
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
				time.Sleep(time.Duration(*timeSwitch) * time.Second)
			case "time":
				formattedTime := timeFormat(zoneName, "15:04")
				screen.WriteData(formattedTime, status)
				time.Sleep(time.Duration(*timeSwitch) * time.Second)
			case "timeBlink":
				ctx, cancel := context.WithTimeout(context.Background(), time.Duration(*timeSwitch)*time.Second)
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
				time.Sleep(time.Duration(*timeSwitch) * time.Second)
			case "string":
				screen.WriteData(*value, status)
				time.Sleep(time.Duration(*timeSwitch) * time.Second)
			case "getByUrl":
				resp, err := http.Get(*url)
				if err != nil {
					fmt.Println("Error:", err)
					continue optionLoop
				}
				body, err := io.ReadAll(resp.Body)
				_ = resp.Body.Close()
				if err != nil {
					fmt.Println("Error reading response body:", err)
					continue optionLoop
				}
				screen.WriteData(string(body), status)
				time.Sleep(time.Duration(*timeSwitch) * time.Second)
			}
		}
	}
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

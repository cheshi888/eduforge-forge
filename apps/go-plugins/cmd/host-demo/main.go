package main

import (
	"fmt"
	"os"
	"time"

	"github.com/eduforge/go-plugins/internal/promo"
)

// host-demo: 无服务形态的热插拔演示, 直接在进程内走宿主侧 Swap + ApplyDiscount 链路。
// 用法: 先编译插件二进制, 再运行本程序 (参数: 插件二进制路径)
func main() {
	if len(os.Args) < 2 {
		fmt.Fprintln(os.Stderr, "usage: host-demo <plugin-binary-path>")
		os.Exit(2)
	}
	bin := os.Args[1]

	host := promo.NewPluginHost()
	name, ok := host.ActiveInfo()
	fmt.Printf("before swap: active=%s external=%v\n", name, ok)

	// 单件 300.00 (300,000,000 micro-unit) x 3 件, 原价 900.00
	req := promo.DiscountRequest{PriceMinor: 300_000_000, Qty: 3}

	start := time.Now()
	if err := host.Swap(bin); err != nil {
		panic(err)
	}
	svg := time.Since(start)

	res, err := host.Apply(req)
	if err != nil {
		panic(err)
	}

	name, ok = host.ActiveInfo()
	original := promo.NewDecimalMinor(res.FinalMinor + res.DiscountMinor)
	final := promo.NewDecimalMinor(res.FinalMinor)
	discount := promo.NewDecimalMinor(res.DiscountMinor)
	fmt.Printf("after swap:  active=%s external=%v\n", name, ok)
	fmt.Printf("swap elapsed: %v (target <10s, no host restart)\n", svg)
	fmt.Printf("original=%s final=%s discount=%s reason=%s applied_by=%s\n",
		original.String(), final.String(), discount.String(), res.Reason, res.Plugin)
}

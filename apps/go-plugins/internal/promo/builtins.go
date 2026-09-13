package promo

import "fmt"

// NoDiscount 无外部插件时的进程内回退实现
type NoDiscount struct{}

func (n NoDiscount) Name() string {
	return "no-discount"
}

func (n NoDiscount) ApplyDiscount(req DiscountRequest) (DiscountResult, error) {
	full, overflow := SafeMul(req.PriceMinor, int64(req.Qty))
	if overflow {
		return DiscountResult{}, fmt.Errorf("price*qty overflow: price=%d qty=%d", req.PriceMinor, req.Qty)
	}
	return DiscountResult{
		FinalMinor: full,
		Reason:     "no active promotion",
		Plugin:     n.Name(),
	}, nil
}

// SecondHalfPrice 内置 "第二件半价" 参考实现:
// 第 2, 4, ... 件按单价 50% 计费, 全程整数微单位运算。
type SecondHalfPrice struct{}

func (s SecondHalfPrice) Name() string {
	return "second-half-price"
}

func (s SecondHalfPrice) ApplyDiscount(req DiscountRequest) (DiscountResult, error) {
	if req.PriceMinor < 0 || req.Qty < 0 {
		return DiscountResult{}, fmt.Errorf("invalid request: price=%d qty=%d", req.PriceMinor, req.Qty)
	}
	fullInt, overflow := SafeMul(req.PriceMinor, int64(req.Qty))
	if overflow {
		return DiscountResult{}, fmt.Errorf("price*qty overflow: price=%d qty=%d", req.PriceMinor, req.Qty)
	}
	full := Decimal{Minor: fullInt}
	if req.Qty < 2 {
		return DiscountResult{
			FinalMinor: full.Minor,
			Plugin:     s.Name(),
			Reason:     "single item, no discount",
		}, nil
	}
	halfCount := req.Qty / 2
	discount := Decimal{Minor: req.PriceMinor}.MulInt64(int64(halfCount)).MulRatio(500_000, 1_000_000)
	return DiscountResult{
		FinalMinor:    full.Sub(discount).Minor,
		DiscountMinor: discount.Minor,
		Reason:        "every 2nd item at 50%",
		Plugin:        s.Name(),
	}, nil
}

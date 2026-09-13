package promo

import (
	"math"
	"testing"
)

// H2-04 回归: 金额×数量溢出必须显式报错, 不允许 int64 回绕出负价
func TestSafeMulOverflow(t *testing.T) {
	if v, ov := SafeMul(math.MaxInt64, 2); !ov {
		t.Fatalf("expected overflow flag, got v=%d", v)
	}
	v, ov := SafeMul(300_000_000, 3)
	if ov || v != 900_000_000 {
		t.Fatalf("300M*3 = %d overflow=%v, want 900000000", v, ov)
	}
}

func TestNoDiscountOverflowRejected(t *testing.T) {
	nd := NoDiscount{}
	if _, err := nd.ApplyDiscount(DiscountRequest{PriceMinor: math.MaxInt64, Qty: 2}); err == nil {
		t.Fatal("expected overflow error")
	}
}

func TestSecondHalfPriceOverflowRejected(t *testing.T) {
	shp := SecondHalfPrice{}
	if _, err := shp.ApplyDiscount(DiscountRequest{PriceMinor: math.MaxInt64, Qty: 2}); err == nil {
		t.Fatal("expected overflow error")
	}
}

// 第二件半价数学回归: 3 件 300.00 → 第 2 件半价, final=750.00 discount=150.00 (微单位)
func TestSecondHalfPriceDiscount(t *testing.T) {
	shp := SecondHalfPrice{}
	res, err := shp.ApplyDiscount(DiscountRequest{PriceMinor: 300_000_000, Qty: 3})
	if err != nil {
		t.Fatal(err)
	}
	if res.FinalMinor != 750_000_000 || res.DiscountMinor != 150_000_000 {
		t.Fatalf("got final=%d discount=%d, want 750000000/150000000", res.FinalMinor, res.DiscountMinor)
	}
	if res.FinalMinor+res.DiscountMinor != 900_000_000 {
		t.Fatal("invariant final+discount == original violated")
	}
}

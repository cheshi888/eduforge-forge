package promo

import (
	"errors"
	"fmt"
	"math"
	"net/rpc"

	"github.com/hashicorp/go-plugin"
)

// Decimal 以 6 位内部精度的最小单位 (micro-unit, 1e-6) 表示金额,
// 全程整数运算, 与 Rust 侧 rust_decimal / DB NUMERIC(20,6) 对齐, 禁用浮点。
type Decimal struct {
	Minor int64
}

func NewDecimalMinor(minor int64) Decimal {
	return Decimal{Minor: minor}
}

// FromCents 从分 (1e-2) 构造 6 位精度 Decimal
func FromCents(cents int64) Decimal {
	return Decimal{Minor: cents * 100_000}
}

func (d Decimal) MulInt64(n int64) Decimal {
	return Decimal{Minor: d.Minor * n}
}

// SafeMul int64 乘法溢出检查 (H2-04: 金额×数量不做 unchecked 回绕)
func SafeMul(a, b int64) (int64, bool) {
	if a > 0 && b > 0 && a > math.MaxInt64/b {
		return 0, true
	}
	return a * b, false
}

// MulRatio 按 num/den 比例缩放, 6 位精度内取整
func (d Decimal) MulRatio(num, den int64) Decimal {
	return Decimal{Minor: d.Minor * num / den}
}

func (d Decimal) Add(o Decimal) Decimal {
	return Decimal{Minor: d.Minor + o.Minor}
}

func (d Decimal) Sub(o Decimal) Decimal {
	return Decimal{Minor: d.Minor - o.Minor}
}

// String 展示层输出: 四舍五入到 2 位小数
func (d Decimal) String() string {
	sign := ""
	if d.Minor < 0 {
		sign = "-"
		d.Minor = -d.Minor
	}
	whole := d.Minor / 1_000_000
	frac := d.Minor % 1_000_000
	round2 := (frac + 5000) / 10000
	if round2 == 100 {
		whole++
		round2 = 0
	}
	return fmt.Sprintf("%s%d.%02d", sign, whole, round2)
}

// DiscountRequest 促销算法输入
type DiscountRequest struct {
	PriceMinor int64 // 单价 (micro-unit)
	Qty        int   // 购买数量
}

// DiscountResult 促销算法输出
type DiscountResult struct {
	FinalMinor    int64  // 折后应付总额
	DiscountMinor int64  // 优惠金额
	Reason        string
	Plugin        string
}

// PromotionInterface 热插拔契约: 所有促销算法插件必须实现该接口。
// 管理端上传新插件 (如 "第二件半价") 时, 宿主进程在 10 秒内原子切换, 全程不重启 Pod。
type PromotionInterface interface {
	Name() string
	ApplyDiscount(req DiscountRequest) (DiscountResult, error)
}

// PingArgs net/rpc 的 Ping 参数占位 (net/rpc 要求参数为 struct 指针)
type PingArgs struct{}

// PromoRPCServer 插件二进制侧的 net/rpc 服务端, 方法注册为 "Plugin.<Method>"
type PromoRPCServer struct {
	Impl PromotionInterface
}

func (s *PromoRPCServer) ApplyDiscount(req *DiscountRequest, resp *DiscountResult) error {
	result, err := s.Impl.ApplyDiscount(*req)
	if err != nil {
		return fmt.Errorf("promotion %s: %w", s.Impl.Name(), err)
	}
	*resp = result
	return nil
}

func (s *PromoRPCServer) Ping(_ *PingArgs, resp *string) error {
	*resp = s.Impl.Name() + ":pong"
	return nil
}

// PromoRemote 宿主侧远端句柄, 通过 net/rpc 调用插件进程
type PromoRemote struct {
	client *rpc.Client
}

func (r *PromoRemote) ApplyDiscount(req DiscountRequest) (DiscountResult, error) {
	var res DiscountResult
	if err := r.client.Call("Plugin.ApplyDiscount", &req, &res); err != nil {
		return DiscountResult{}, err
	}
	return res, nil
}

func (r *PromoRemote) Ping() error {
	var pong string
	return r.client.Call("Plugin.Ping", &PingArgs{}, &pong)
}

// PromoPlugin 宿主侧 plugin.Plugin 实现 (netrpc 模式)
type PromoPlugin struct{}

func (p *PromoPlugin) Server(_ *plugin.MuxBroker) (interface{}, error) {
	// 服务端由插件二进制的 PromoRPCServer 提供
	return nil, errors.New("netrpc server is provided by the plugin binary")
}

func (p *PromoPlugin) Client(_ *plugin.MuxBroker, raw *rpc.Client) (interface{}, error) {
	return &PromoRemote{client: raw}, nil
}

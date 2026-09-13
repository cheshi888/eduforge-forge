package main

import (
	"errors"
	"net/rpc"

	"github.com/hashicorp/go-plugin"

	"github.com/eduforge/go-plugins/internal/promo"
)

// 可独立编译的促销插件二进制 ("第二件半价" 示例)。
// 宿主进程通过 go-plugin 的 net/rpc 传输协议动态加载, 热切换时仅替换该二进制, Pod 不重启。

var handshake = plugin.HandshakeConfig{
	ProtocolVersion:  1,
	MagicCookieKey:   "EDUFORGE",
	MagicCookieValue: "eduforge-neo",
}

// serverAdapter 把 promo.PromoRPCServer (net/rpc 服务端) 挂进 go-plugin 框架
type serverAdapter struct{}

func (a *serverAdapter) Server(_ *plugin.MuxBroker) (interface{}, error) {
	return &promo.PromoRPCServer{Impl: promo.SecondHalfPrice{}}, nil
}

func (a *serverAdapter) Client(_ *plugin.MuxBroker, _ *rpc.Client) (interface{}, error) {
	return nil, errors.New("client side is host-only")
}

func main() {
	plugin.Serve(&plugin.ServeConfig{
		HandshakeConfig: handshake,
		Plugins: map[string]plugin.Plugin{
			"promotion": &serverAdapter{},
		},
	})
}

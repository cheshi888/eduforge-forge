package promo

import (
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sync"
	"time"

	"github.com/hashicorp/go-plugin"
	"go.opentelemetry.io/otel"
	"go.opentelemetry.io/otel/attribute"
	"go.opentelemetry.io/otel/codes"
)

var tracer = otel.Tracer("eduforge-go-plugins")

var handshake = plugin.HandshakeConfig{
	ProtocolVersion:  1,
	MagicCookieKey:   "EDUFORGE",
	MagicCookieValue: "eduforge-neo",
}

const pluginKey = "promotion"

type activePlugin struct {
	client *plugin.Client
	remote *PromoRemote
	path   string
	name   string
}

// PluginHost 管理插件进程的生命周期: 启动 → 握手 → 健康检查 → 原子切换 → 回收旧进程。
// 切换在锁内完成指针交换, 读侧 (ApplyDiscount) 感知不到停机窗口。
type PluginHost struct {
	mu     sync.RWMutex
	active *activePlugin
}

func NewPluginHost() *PluginHost {
	return &PluginHost{}
}

func (h *PluginHost) spawn(binPath string) (*activePlugin, error) {
	client := plugin.NewClient(&plugin.ClientConfig{
		HandshakeConfig: handshake,
		Plugins:         map[string]plugin.Plugin{pluginKey: &PromoPlugin{}},
		Cmd:             exec.Command(binPath),
		AllowedProtocols: []plugin.Protocol{
			plugin.ProtocolNetRPC,
		},
		Stderr: &logWriter{prefix: "[plugin:" + filepath.Base(binPath) + "]"},
	})
	crpc, err := client.Client()
	if err != nil {
		client.Kill()
		return nil, fmt.Errorf("plugin handshake: %w", err)
	}
	dispensed, err := crpc.Dispense(pluginKey)
	if err != nil {
		crpc.Close()
		client.Kill()
		return nil, fmt.Errorf("plugin dispense: %w", err)
	}
	remote, ok := dispensed.(*PromoRemote)
	if !ok {
		crpc.Close()
		client.Kill()
		return nil, errors.New("plugin dispense type mismatch")
	}
	if err := remote.Ping(); err != nil {
		crpc.Close()
		client.Kill()
		return nil, fmt.Errorf("plugin health check: %w", err)
	}
	return &activePlugin{client: client, remote: remote, path: binPath, name: filepath.Base(binPath)}, nil
}

// Swap 加载新插件二进制并热切换, 目标 < 10s, 全程不重启宿主进程 (Pod 无需重启)。
func (h *PluginHost) Swap(binPath string) error {
	_, span := tracer.Start(context.Background(), "plugins.swap")
	defer span.End()
	span.SetAttributes(attribute.String("plugin.path", binPath))

	candidate, err := h.spawn(binPath)
	if err != nil {
		span.SetStatus(codes.Error, err.Error())
		return err
	}

	start := time.Now()
	h.mu.Lock()
	old := h.active
	h.active = candidate
	h.mu.Unlock()
	if old != nil {
		old.client.Kill()
	}
	elapsed := time.Since(start)
	span.SetAttributes(attribute.Float64("plugins.swap.elapsed_ms", float64(elapsed.Microseconds())/1000.0))
	return nil
}

// rpcTimeout H2-04: 插件 RPC 客户端超时 — 挂起/恶意插件不再造成 goroutine 堆积 (公开端点 DoS 面)
const rpcTimeout = 5 * time.Second

type applyDone struct {
	res DiscountResult
	err error
}

// Apply 走当前激活插件的折扣算法; 无激活插件、调用失败或超时 (5s) 时回退进程内实现。
func (h *PluginHost) Apply(req DiscountRequest) (DiscountResult, error) {
	_, span := tracer.Start(context.Background(), "plugins.applyDiscount")
	defer span.End()

	h.mu.RLock()
	ap := h.active
	h.mu.RUnlock()

	if ap == nil {
		span.SetAttributes(attribute.String("plugins.active", NoDiscount{}.Name()))
		return NoDiscount{}.ApplyDiscount(req)
	}
	done := make(chan applyDone, 1)
	go func() {
		res, err := ap.remote.ApplyDiscount(req)
		done <- applyDone{res: res, err: err}
	}()
	select {
	case d := <-done:
		if d.err != nil {
			span.SetStatus(codes.Error, d.err.Error())
			return NoDiscount{}.ApplyDiscount(req)
		}
		d.res.Plugin = ap.name
		return d.res, nil
	case <-time.After(rpcTimeout):
		span.SetStatus(codes.Error, "plugin rpc timeout (5s), fallback to builtin")
		return NoDiscount{}.ApplyDiscount(req)
	}
}

func (h *PluginHost) ActiveInfo() (string, bool) {
	h.mu.RLock()
	defer h.mu.RUnlock()
	if h.active == nil {
		return "no-discount (builtin)", false
	}
	return h.active.name, true
}

type logWriter struct {
	prefix string
}

func (w *logWriter) Write(p []byte) (int, error) {
	for _, line := range splitLines(p) {
		if len(line) > 0 {
			fmt.Fprintln(os.Stderr, w.prefix, string(line))
		}
	}
	return len(p), nil
}

func splitLines(data []byte) [][]byte {
	var out [][]byte
	start := 0
	for i, b := range data {
		if b == '\n' {
			out = append(out, data[start:i])
			start = i + 1
		}
	}
	if start < len(data) {
		out = append(out, data[start:])
	}
	return out
}

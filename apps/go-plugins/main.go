package main

import (
	"context"
	"crypto/subtle"
	"log"
	"net/http"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"time"

	"github.com/gin-gonic/gin"
	otelgin "go.opentelemetry.io/contrib/instrumentation/github.com/gin-gonic/gin/otelgin"
	"go.opentelemetry.io/otel"
	"go.opentelemetry.io/otel/exporters/otlp/otlptrace/otlptracegrpc"
	"go.opentelemetry.io/otel/propagation"
	"go.opentelemetry.io/otel/sdk/resource"
	sdktrace "go.opentelemetry.io/otel/sdk/trace"
	semconv "go.opentelemetry.io/otel/semconv/v1.25.0"

	"github.com/eduforge/go-plugins/internal/promo"
)

// B-03 修复: 管理端上传通道安全基线。
// 1) adminAuth: FORGE_ADMIN_TOKEN 未配置时 fail-closed 拒绝所有上传 (安全默认);
// 2) plugin_name 白名单 ^[a-z0-9_-]{1,64}$, 杜绝 ".." 路径穿越;
// 3) 落盘前 Abs+前缀校验做纵深防御。
var pluginNameRe = regexp.MustCompile(`^[a-z0-9_-]{1,64}$`)

// adminAuth 校验 Authorization: Bearer <FORGE_ADMIN_TOKEN>, 恒定时间比较。
func adminAuth(c *gin.Context) {
	token := os.Getenv("FORGE_ADMIN_TOKEN")
	if token == "" {
		c.AbortWithStatusJSON(http.StatusServiceUnavailable,
			gin.H{"error": "admin upload disabled: FORGE_ADMIN_TOKEN not configured"})
		return
	}
	provided := strings.TrimPrefix(c.GetHeader("Authorization"), "Bearer ")
	if subtle.ConstantTimeCompare([]byte(provided), []byte(token)) != 1 {
		c.AbortWithStatusJSON(http.StatusUnauthorized, gin.H{"error": "unauthorized"})
		return
	}
	c.Next()
}
type Server struct {
	host      *promo.PluginHost
	pluginDir string
}

func main() {
	tp := initOtel()

	host := promo.NewPluginHost()
	srv := &Server{host: host, pluginDir: envOr("FORGE_PLUGIN_DIR", "/var/lib/eduforge/plugins")}
	if err := os.MkdirAll(srv.pluginDir, 0o755); err != nil {
		log.Fatal(err)
	}
	if bin := os.Getenv("FORGE_PLUGIN_BIN"); bin != "" {
		if err := host.Swap(bin); err != nil {
			log.Printf("startup plugin load failed: %v (fallback to builtin)", err)
		}
	}

	r := gin.New()
	r.Use(gin.Logger(), gin.Recovery(), otelgin.Middleware("eduforge-go-plugins", otelgin.WithTracerProvider(tp)))
	r.POST("/admin/plugins", adminAuth, srv.adminUpload)
	r.GET("/plugins/active", srv.active)
	// H2-04: preview 收口到管理端鉴权 — 公开无鉴权端点可被 DoS 循环放大 (插件挂起),
	// 且无消费方依赖公开读 (web 结算走 rust-core /internal/checkout)
	r.POST("/discount/preview", adminAuth, srv.discountPreview)

	addr := envOr("FORGE_PLUGIN_BIND", "0.0.0.0:3001")
	log.Printf("eduforge go-plugins listening on %s", addr)
	if err := http.ListenAndServe(addr, r); err != nil {
		log.Fatal(err)
	}
}

// adminUpload 管理端上传新插件二进制, 校验 → 健康检查 → 原子热切换。
func (s *Server) adminUpload(c *gin.Context) {
	form, err := c.MultipartForm()
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "multipart parse: " + err.Error()})
		return
	}
	name := c.PostForm("plugin_name")
	if name == "" {
		name = "promotion"
	}
	if !pluginNameRe.MatchString(name) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "plugin_name must match ^[a-z0-9_-]{1,64}$"})
		return
	}
	files := form.File["artifact"]
	if len(files) == 0 {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing artifact file"})
		return
	}
	f, err := files[0].Open()
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	defer f.Close()

	dst := filepath.Join(s.pluginDir, name)
	// Defense in depth: name already restricted by pluginNameRe (no '/', no '..'),
	// but verify the joined path stays under pluginDir before writing.
	absDir, err := filepath.Abs(s.pluginDir)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "plugin dir resolve: " + err.Error()})
		return
	}
	absDst, err := filepath.Abs(dst)
	if err != nil || absDst != absDir && !strings.HasPrefix(absDst, absDir+string(os.PathSeparator)) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "plugin path escapes plugin dir"})
		return
	}
	if err := c.SaveUploadedFile(files[0], dst); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	if err := os.Chmod(dst, 0o755); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	start := time.Now()
	if err := s.host.Swap(dst); err != nil {
		c.JSON(http.StatusUnprocessableEntity, gin.H{"error": "plugin rejected: " + err.Error()})
		return
	}
	elapsed := time.Since(start)
	active, _ := s.host.ActiveInfo()
	c.JSON(http.StatusOK, gin.H{
		"swapped":       true,
		"active_plugin": active,
		"duration_ms":   elapsed.Milliseconds(),
	})
}

func (s *Server) active(c *gin.Context) {
	name, external := s.host.ActiveInfo()
	c.JSON(http.StatusOK, gin.H{"name": name, "external": external})
}

func (s *Server) discountPreview(c *gin.Context) {
	var req struct {
		PriceMinor int64 `json:"price_minor"`
		Qty        int   `json:"qty"`
	}
	if err := c.ShouldBindJSON(&req); err != nil || req.PriceMinor < 0 || req.Qty < 0 || req.Qty > 10000 {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid request: price_minor and qty required, non-negative, qty<=10000"})
		return
	}
	full, overflow := promo.SafeMul(req.PriceMinor, int64(req.Qty))
	if overflow {
		c.JSON(http.StatusBadRequest, gin.H{"error": "price*qty overflow"})
		return
	}
	res, err := s.host.Apply(promo.DiscountRequest{PriceMinor: req.PriceMinor, Qty: req.Qty})
	if err != nil {
		c.JSON(http.StatusBadGateway, gin.H{"error": err.Error()})
		return
	}
	// H2-04: 插件结果边界校验 — 负价/超额折扣/不守恒结果一律拒显, 防有 bug 的已上传插件污染定价
	if res.FinalMinor < 0 || res.DiscountMinor < 0 || res.FinalMinor+res.DiscountMinor != full {
		c.JSON(http.StatusBadRequest, gin.H{"error": "plugin returned out-of-bound price"})
		return
	}
	c.JSON(http.StatusOK, gin.H{
		"final_minor":     res.FinalMinor,
		"discount_minor":  res.DiscountMinor,
		"plugin":          res.Plugin,
		"reason":          res.Reason,
		"final_display":   promo.NewDecimalMinor(res.FinalMinor).String(),
		"discount_display": promo.NewDecimalMinor(res.DiscountMinor).String(),
	})
}

func initOtel() *sdktrace.TracerProvider {
	endpoint := os.Getenv("OTEL_EXPORTER_OTLP_ENDPOINT")
	if endpoint == "" {
		tp := sdktrace.NewTracerProvider()
		otel.SetTracerProvider(tp)
		return tp
	}
	insecure := strings.HasPrefix(endpoint, "http://")
	host := endpoint
	if i := strings.Index(host, "://"); i >= 0 {
		host = host[i+len("://"):]
	}
	opts := []otlptracegrpc.Option{otlptracegrpc.WithEndpoint(host)}
	if insecure {
		opts = append(opts, otlptracegrpc.WithInsecure())
	}
	exp, err := otlptracegrpc.New(context.Background(), opts...)
	if err != nil {
		log.Printf("otel exporter init failed: %v, using noop", err)
		tp := sdktrace.NewTracerProvider()
		otel.SetTracerProvider(tp)
		return tp
	}
	tp := sdktrace.NewTracerProvider(
		sdktrace.WithBatcher(exp),
		sdktrace.WithResource(resource.NewWithAttributes(
			semconv.SchemaURL,
			semconv.ServiceName("eduforge-go-plugins"),
		)),
	)
	otel.SetTracerProvider(tp)
	otel.SetTextMapPropagator(propagation.TraceContext{})
	return tp
}

func envOr(key, fallback string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return fallback
}

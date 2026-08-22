//go:build ignore

// quickstart_stats — Probe service health/stats via GET /api/health.
//
// Customer concept: liveness + readiness probe. Returns version,
// region, and per-dependency status. Anonymous endpoint.
//
// Run: go run examples/quickstart_stats.go
package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"time"
)

func main() {
	api := os.Getenv("CORELINK_API_URL")
	if api == "" {
		api = "https://corelink-api.humangr.com"
	}
	req, err := http.NewRequest("GET", api+"/api/health", nil)
	if err != nil {
		fmt.Fprintf(os.Stderr, "build request: %v\n", err)
		os.Exit(1)
	}
	req.Header.Set("Accept", "application/json")
	client := &http.Client{Timeout: 10 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		fmt.Fprintf(os.Stderr, "http: %v\n", err)
		os.Exit(1)
	}
	defer resp.Body.Close()
	raw, _ := io.ReadAll(resp.Body)
	if resp.StatusCode/100 != 2 {
		fmt.Fprintf(os.Stderr, "server returned %d: %s\n", resp.StatusCode, string(raw))
		os.Exit(1)
	}
	var pretty bytes.Buffer
	if err := json.Indent(&pretty, raw, "", "  "); err != nil {
		pretty.Write(raw)
	}
	fmt.Printf("status=%d\n%s\n", resp.StatusCode, pretty.String())
}

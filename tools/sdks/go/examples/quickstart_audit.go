//go:build ignore

// quickstart_audit — Stream audit events via GET /v1/admin/audit-events.
//
// Customer concept: every privileged op is Merkle-chained into the
// audit log (INV-AUDIT-APPEND-ONLY). Response includes chain_head_hash
// for client-side verification (CTRL-AUDIT-002).
//
// Run: CORELINK_PAT=$ADMIN_PAT go run examples/quickstart_audit.go
package main

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"time"
)

func main() {
	api := os.Getenv("CORELINK_API_URL")
	if api == "" {
		api = "https://sandbox.corelink.dev"
	}
	pat := os.Getenv("CORELINK_PAT")
	if pat == "" {
		fmt.Fprintln(os.Stderr, "error: CORELINK_PAT env var is required")
		os.Exit(2)
	}
	u, err := url.Parse(api + "/v1/admin/audit-events")
	if err != nil {
		fmt.Fprintf(os.Stderr, "parse url: %v\n", err)
		os.Exit(1)
	}
	q := u.Query()
	q.Set("limit", "50")
	u.RawQuery = q.Encode()
	req, err := http.NewRequest("GET", u.String(), nil)
	if err != nil {
		fmt.Fprintf(os.Stderr, "build request: %v\n", err)
		os.Exit(1)
	}
	req.Header.Set("Authorization", "Bearer "+pat)
	req.Header.Set("Accept", "application/json")
	client := &http.Client{Timeout: 10 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		fmt.Fprintf(os.Stderr, "http: %v\n", err)
		os.Exit(1)
	}
	defer resp.Body.Close()
	raw, _ := io.ReadAll(resp.Body)
	var decoded map[string]any
	events, head := 0, "<none>"
	if err := json.Unmarshal(raw, &decoded); err == nil {
		if e, ok := decoded["events"].([]any); ok {
			events = len(e)
		}
		if h, ok := decoded["chain_head_hash"].(string); ok {
			head = h
		}
	}
	fmt.Printf("status=%d events=%d chain_head=%s\n", resp.StatusCode, events, head)
}

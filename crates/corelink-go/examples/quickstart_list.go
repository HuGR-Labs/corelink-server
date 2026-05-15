//go:build ignore

// quickstart_list — List all PATs via GET /v1/pats.
//
// Customer concept: server-side authoritative credential inventory.
// Plaintext tokens are never returned (only metadata + last-4 prefix).
//
// Run: CORELINK_PAT=$PAT go run examples/quickstart_list.go
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
		api = "https://sandbox.corelink.dev"
	}
	pat := os.Getenv("CORELINK_PAT")
	if pat == "" {
		fmt.Fprintln(os.Stderr, "error: CORELINK_PAT env var is required")
		os.Exit(2)
	}
	req, err := http.NewRequest("GET", api+"/v1/pats", nil)
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
	if resp.StatusCode/100 != 2 {
		fmt.Fprintf(os.Stderr, "server returned %d: %s\n", resp.StatusCode, string(raw))
		os.Exit(1)
	}
	var decoded map[string]any
	count := 0
	if err := json.Unmarshal(raw, &decoded); err == nil {
		if items, ok := decoded["items"].([]any); ok {
			count = len(items)
		}
	}
	var pretty bytes.Buffer
	_ = json.Indent(&pretty, raw, "", "  ")
	fmt.Printf("status=%d pat_count=%d\n%s\n", resp.StatusCode, count, pretty.String())
}

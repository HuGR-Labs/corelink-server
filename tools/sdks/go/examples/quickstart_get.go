//go:build ignore

// quickstart_get — Fetch the authenticated principal via GET /v1/users/me.
//
// Customer concept: canonical "who am I" probe. Verify PAT validity,
// scopes, and tenant binding before issuing follow-up calls.
//
// Run: CORELINK_PAT=$PAT go run examples/quickstart_get.go
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
	req, err := http.NewRequest("GET", api+"/v1/users/me", nil)
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
	var pretty bytes.Buffer
	if err := json.Indent(&pretty, raw, "", "  "); err != nil {
		pretty.Write(raw)
	}
	fmt.Printf("status=%d\n%s\n", resp.StatusCode, pretty.String())
}

//go:build ignore

// quickstart_signup — Provision a new CoreLink tenant via POST /v1/signup.
//
// Customer concept: a tenant is the top-level isolation boundary in
// CoreLink. Signup is atomic: tenant row + DPA acceptance + first PAT
// are committed together (INV-ONBOARD-ATOMIC-PROVISIONING).
//
// Run: CORELINK_PAT=$PAT go run examples/quickstart_signup.go
package main

import (
	"bytes"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"time"
)

func randomID() string {
	var b [16]byte
	if _, err := rand.Read(b[:]); err != nil {
		return "fallback-id"
	}
	return hex.EncodeToString(b[:])
}

func main() {
	api := os.Getenv("CORELINK_API_URL")
	if api == "" {
		api = "https://corelink-api.humangr.com"
	}
	pat := os.Getenv("CORELINK_PAT")
	if pat == "" {
		fmt.Fprintln(os.Stderr, "error: CORELINK_PAT env var is required")
		os.Exit(2)
	}
	idem := "idem-" + randomID()
	body, err := json.Marshal(map[string]any{
		"clerk_event_id":  "evt_demo_quickstart",
		"correlation_id":  randomID(),
		"email_hash":      "0000000000000000000000000000000000000000000000000000000000000000",
		"idempotency_key": idem,
		"locale":          "en-US",
	})
	if err != nil {
		fmt.Fprintf(os.Stderr, "marshal: %v\n", err)
		os.Exit(1)
	}
	req, err := http.NewRequest("POST", api+"/v1/signup", bytes.NewReader(body))
	if err != nil {
		fmt.Fprintf(os.Stderr, "build request: %v\n", err)
		os.Exit(1)
	}
	req.Header.Set("Authorization", "Bearer "+pat)
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Idempotency-Key", idem)

	client := &http.Client{Timeout: 10 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		fmt.Fprintf(os.Stderr, "http: %v\n", err)
		os.Exit(1)
	}
	defer resp.Body.Close()
	out, _ := io.ReadAll(resp.Body)
	fmt.Printf("status=%d body=%s\n", resp.StatusCode, string(out))
}

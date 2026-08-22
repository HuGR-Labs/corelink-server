//go:build ignore

// quickstart_portal_session — Bootstrap an enterprise-portal session
// via POST /v1/enterprise/inquire.
//
// Customer concept: enterprise prospects request contracted-tier
// provisioning (DPA + BAA + custom SLA). Returns a portal URL +
// short-lived session token.
//
// Run: CORELINK_PAT=$PAT go run examples/quickstart_portal_session.go
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
		"company":            "Acme Corp",
		"contact_email_hash": "0000000000000000000000000000000000000000000000000000000000000000",
		"expected_seats":     250,
		"use_case":           "monorepo build cache for 80 engineers",
	})
	if err != nil {
		fmt.Fprintf(os.Stderr, "marshal: %v\n", err)
		os.Exit(1)
	}
	req, err := http.NewRequest("POST", api+"/v1/enterprise/inquire", bytes.NewReader(body))
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

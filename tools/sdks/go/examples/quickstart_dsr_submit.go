//go:build ignore

// quickstart_dsr_submit — Submit a DSR via POST /v1/privacy/dsr/{action}.
//
// Customer concept: GDPR/CCPA data-subject requests. Returns a
// request_id; poll GET /v1/privacy/dsr/{request_id}/status for
// progress. Erasure produces a signed attestation
// (CTRL-ERASURE-ATTEST-001).
//
// Run: CORELINK_PAT=$PAT DSR_ACTION=export go run examples/quickstart_dsr_submit.go
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
	action := os.Getenv("DSR_ACTION")
	if action == "" {
		action = "export"
	}
	idem := "idem-" + randomID()
	body, err := json.Marshal(map[string]any{
		"subject_email_hash": "0000000000000000000000000000000000000000000000000000000000000000",
		"verification_token": "demo-verification-token",
		"scope":              []string{"profile", "audit_events"},
	})
	if err != nil {
		fmt.Fprintf(os.Stderr, "marshal: %v\n", err)
		os.Exit(1)
	}
	req, err := http.NewRequest("POST", api+"/v1/privacy/dsr/"+action, bytes.NewReader(body))
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
	fmt.Printf("action=%s status=%d body=%s\n", action, resp.StatusCode, string(out))
}

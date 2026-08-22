//go:build ignore

// quickstart_team_invite — File a team-invite op via POST /v1/admin/ops.
//
// Customer concept: adding tenant members flows through the admin-ops
// dual-approval gate (CTRL-DUAL-APPROVAL-001). On approval an invite
// email is dispatched via the verified-sender pipeline.
//
// Run: CORELINK_PAT=$ADMIN_PAT INVITEE_EMAIL=alice@example.com \
//        go run examples/quickstart_team_invite.go
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
	invitee := os.Getenv("INVITEE_EMAIL")
	if invitee == "" {
		invitee = "newmember@example.com"
	}
	idem := "idem-" + randomID()
	body, err := json.Marshal(map[string]any{
		"op_type": "team_invite",
		"reason":  "onboard new engineer",
		"params":  map[string]any{"email": invitee, "role": "developer"},
	})
	if err != nil {
		fmt.Fprintf(os.Stderr, "marshal: %v\n", err)
		os.Exit(1)
	}
	req, err := http.NewRequest("POST", api+"/v1/admin/ops", bytes.NewReader(body))
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

// CoreLink Go SDK quickstart — corelink-go.
//
// Requires:
//
//	go get github.com/humangr-labs/corelink-go/v1
//	export CORELINK_PAT=<your-pat>
//	cargo build --release -p corelink-go
//	CGO_LDFLAGS="-L../../target/release -lcorelink_go" go run quickstart.go
package main

import (
	"context"
	"fmt"
	"os"

	corelink "github.com/humangr-labs/corelink-go/v1"
)

func main() {
	pat := os.Getenv("CORELINK_PAT")
	if pat == "" {
		fmt.Fprintln(os.Stderr, "ERROR: CORELINK_PAT environment variable not set")
		os.Exit(1)
	}

	// ClientVerify defaults to true per CTRL-CAS-002 (zero-value guard).
	client, err := corelink.NewClient(corelink.Config{
		PAT:      pat,
		TenantID: "acme-corp",
		// ClientVerify: true  // default; set ClientVerifyExplicitFalse=true to opt out
	})
	if err != nil {
		panic(fmt.Sprintf("NewClient: %v", err))
	}
	defer client.Close()

	fmt.Printf("IsClientVerifyEnabled: %v\n", client.IsClientVerifyEnabled())

	ctx := context.Background()

	// Put
	data := []byte("hello from CoreLink Go SDK")
	digest, err := client.Put(ctx, data)
	if err != nil {
		panic(fmt.Sprintf("Put: %v", err))
	}
	fmt.Printf("Uploaded:  blake3:%s\n", digest)

	// Get (client-verify enforced by Rust single truth)
	downloaded, err := client.Get(ctx, digest)
	if err != nil {
		panic(fmt.Sprintf("Get: %v", err))
	}
	fmt.Printf("Downloaded: %d bytes\n", len(downloaded))

	// Stat
	stat, err := client.Stat(ctx, digest)
	if err != nil {
		panic(fmt.Sprintf("Stat: %v", err))
	}
	fmt.Printf("Stat:      digest=%s... size=%d exists=%v\n", stat.Digest[:8], stat.SizeBytes, stat.Exists)

	fmt.Println("OK — Go SDK quickstart complete.")
}

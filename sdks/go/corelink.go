// Package corelink is the Go client for CoreLink, a multi-tenant
// content-addressable cache and storage-governance platform.
//
// This file is the frozen API surface. It exists because the documentation
// for this SDK was written before the code, so the documentation is the
// specification: every identifier below already appears in a published page
// under apps/docs/docs/how-to/sdk-go or reference/reapi/_examples. Changing a
// name here silently breaks a page.
//
// Implementations live in sibling files. This file declares types only.
package corelink

import (
	"net/http"
	"sync"
	"time"
)

// DigestFunction selects the hash a client uses for content addressing.
// CoreLink's CAS is BLAKE3: a SHA-256 digest is rejected with 422
// HashMismatch on write. SHA-256 exists for REAPI instances configured for it.
type DigestFunction int

const (
	// DigestBLAKE3 is the CAS default and the only function the native
	// /v1/cas surface accepts.
	DigestBLAKE3 DigestFunction = iota
	// DigestSHA256 is for REAPI instances that negotiate it via Capabilities.
	DigestSHA256
)

// Digest identifies content by hash and size. Hex is lower-case, 64 characters.
type Digest struct {
	Hash      string
	SizeBytes int64
}

// Config configures a Client. Endpoint and PAT are required; a PAT carries its
// own tenant, so TenantID is optional and, when set, must match it — the edge
// rejects a mismatch rather than silently using one of the two.
type Config struct {
	Endpoint   string
	PAT        string
	TenantID   string
	Instance   string
	Digest     DigestFunction
	HTTPClient *http.Client
	Timeout    time.Duration
	UserAgent  string

	// ClientVerify re-hashes every blob the client reads before returning it,
	// which is what stops a poisoned cache entry from reaching the caller. It
	// defaults to true, so the zero value is the safe one; opting out needs
	// ClientVerifyExplicitFalse, because a plain false is indistinguishable
	// from "not set".
	ClientVerify              bool
	ClientVerifyExplicitFalse bool
}

// Option mutates a Config. NewClient applies options after the base Config.
type Option func(*Config)

// WithTenantID pins the tenant explicitly instead of taking the PAT's own.
func WithTenantID(id string) Option { return func(c *Config) { c.TenantID = id } }

// Client is a CoreLink client. It is safe for concurrent use.
type Client struct {
	mu   sync.RWMutex
	cfg  Config
	http *http.Client
}

// Stat reports what the CAS knows about a blob without transferring it.
type Stat struct {
	Exists    bool
	Digest    string
	SizeBytes int64
}

// ActionResult is a REAPI v2 action-cache entry.
type ActionResult struct {
	ExitCode    int32
	StdoutRaw   []byte
	OutputFiles []OutputFile
}

// OutputFile is one file produced by a cached action.
type OutputFile struct {
	Path         string
	Digest       Digest
	IsExecutable bool
}

// ServerCapabilities is what a REAPI instance reports it supports.
type ServerCapabilities struct {
	DigestFunctions   []DigestFunction
	MaxBatchTotalSize int64
	SymlinkPolicy     string
}

// HealthStatus is the server's own view of its readiness.
type HealthStatus struct {
	OK      bool
	Region  string
	Version string
}

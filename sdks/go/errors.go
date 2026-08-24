package corelink

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"strconv"
	"time"
)

// ErrActionCacheMiss is returned by ActionCache.Get when the action digest
// is not present in the cache. It is a sentinel: callers match it with
// errors.Is, not a type assertion, since a miss carries no extra detail.
var ErrActionCacheMiss = errors.New("corelink: action cache miss")

// AuthError means the PAT was rejected, missing, or its tenant disagreed
// with the request's URL tenant -- the edge rejects that mismatch rather
// than silently picking one.
type AuthError struct {
	StatusCode int
	Message    string
}

func (e *AuthError) Error() string {
	return fmt.Sprintf("corelink: auth error (%d): %s", e.StatusCode, e.Message)
}

// QuotaError means the tenant's byte or request quota is exhausted.
type QuotaError struct {
	StatusCode int
	Message    string
}

func (e *QuotaError) Error() string {
	return fmt.Sprintf("corelink: quota exceeded (%d): %s", e.StatusCode, e.Message)
}

// RateLimitError means the caller is being throttled. RetryAfter is the
// server's advertised backoff, when it sent one.
type RateLimitError struct {
	StatusCode int
	Message    string
	RetryAfter time.Duration
}

func (e *RateLimitError) Error() string {
	return fmt.Sprintf("corelink: rate limited (%d): %s", e.StatusCode, e.Message)
}

// RegionError means the request landed on, or asked for, a region that
// cannot serve this tenant or object.
type RegionError struct {
	StatusCode int
	Message    string
	Region     string
}

func (e *RegionError) Error() string {
	return fmt.Sprintf("corelink: region error (%d) region=%q: %s", e.StatusCode, e.Region, e.Message)
}

// NotFoundError means the requested digest, action, or resource does not
// exist in the tenant's store.
type NotFoundError struct {
	StatusCode int
	Message    string
}

func (e *NotFoundError) Error() string {
	return fmt.Sprintf("corelink: not found (%d): %s", e.StatusCode, e.Message)
}

// DigestMismatchError means the content hashed to a different digest than
// expected -- either the caller sent the wrong digest for a Put, or a Get's
// client-side verification caught a corrupted or poisoned read.
type DigestMismatchError struct {
	Expected Digest
	Actual   Digest
}

func (e *DigestMismatchError) Error() string {
	return fmt.Sprintf("corelink: digest mismatch: expected %s, got %s", e.Expected, e.Actual)
}

// apiErrorBody is the best-effort shape of a CoreLink JSON error body. Every
// field is optional: a response with no body, or a non-JSON body, still
// produces a typed error from the status code alone.
type apiErrorBody struct {
	Error   string `json:"error"`
	Message string `json:"message"`
	Code    string `json:"code"`
	Region  string `json:"region"`
}

// errorFromResponse turns an HTTP status plus response body into the
// matching typed error. cas.go and reapi.go both call this after do()
// returns a non-2xx response, so they share one place that knows how to
// read a CoreLink error body.
//
// It reads and closes resp.Body; callers must not read resp.Body again
// afterward.
func errorFromResponse(resp *http.Response) error {
	defer resp.Body.Close()
	body, _ := io.ReadAll(resp.Body)

	var parsed apiErrorBody
	_ = json.Unmarshal(body, &parsed)

	msg := parsed.Message
	if msg == "" {
		msg = parsed.Error
	}
	if msg == "" {
		msg = string(body)
	}
	if msg == "" {
		msg = http.StatusText(resp.StatusCode)
	}

	switch resp.StatusCode {
	case http.StatusUnauthorized, http.StatusForbidden:
		return &AuthError{StatusCode: resp.StatusCode, Message: msg}
	case http.StatusPaymentRequired:
		return &QuotaError{StatusCode: resp.StatusCode, Message: msg}
	case http.StatusTooManyRequests:
		return &RateLimitError{
			StatusCode: resp.StatusCode,
			Message:    msg,
			RetryAfter: retryAfter(resp),
		}
	case http.StatusNotFound:
		return &NotFoundError{StatusCode: resp.StatusCode, Message: msg}
	case http.StatusUnprocessableEntity:
		// The server's 422 body doesn't carry structured Expected/Actual
		// digests for us to populate a DigestMismatchError with, so we
		// surface a generic message here. Callers that already know the
		// expected digest (cas.go's Put, digest.go's VerifyBytes) construct
		// a fully-populated *DigestMismatchError themselves instead of
		// going through this path.
		return &statusMessageError{kind: "digest mismatch", statusCode: resp.StatusCode, message: msg}
	case http.StatusMisdirectedRequest, http.StatusTemporaryRedirect:
		return &RegionError{StatusCode: resp.StatusCode, Message: msg, Region: parsed.Region}
	default:
		if parsed.Code == "region_mismatch" || parsed.Code == "tenant_region_mismatch" {
			return &RegionError{StatusCode: resp.StatusCode, Message: msg, Region: parsed.Region}
		}
		return fmt.Errorf("corelink: request failed (%d): %s", resp.StatusCode, msg)
	}
}

// statusMessageError is a small internal fallback for cases where a typed
// error field (like DigestMismatchError's Expected/Actual) can't be filled
// in from the generic error body alone.
type statusMessageError struct {
	kind       string
	statusCode int
	message    string
}

func (e *statusMessageError) Error() string {
	return fmt.Sprintf("corelink: %s (%d): %s", e.kind, e.statusCode, e.message)
}

// retryAfter parses the standard Retry-After header, which the server may
// send as either an integer number of seconds or an HTTP date.
func retryAfter(resp *http.Response) time.Duration {
	v := resp.Header.Get("Retry-After")
	if v == "" {
		return 0
	}
	if secs, err := strconv.Atoi(v); err == nil {
		return time.Duration(secs) * time.Second
	}
	if t, err := http.ParseTime(v); err == nil {
		d := time.Until(t)
		if d < 0 {
			return 0
		}
		return d
	}
	return 0
}

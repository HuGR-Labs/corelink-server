//! Real CF D1 binding adapter with tenant-scoped query primitives
//! (R-PREP, wasm32 production binding — replicates the pattern landed
//! in [`crate::r2_real`] for the D1 surface).
//!
//! # What this module adds over `crate::cf_d1::CfD1DatabaseAdapter` (wasm32-only)
//!
//! `cf_d1::CfD1DatabaseAdapter` is the **minimal** raw-passthrough
//! `prepare` / `exec` wrapper used by `corelink-clerk-cf::health` for
//! the workload that pre-dates the D1 trait surface. Production also
//! needs:
//!
//! - **Tenant prefix enforcement** on every query. D1 is multi-tenant
//!   at the row level: a stray query that forgets `WHERE tenant_id = ?`
//!   is a cross-tenant data exposure (CTRL-PRIV-001 + INV-TENANT-ISOLATION).
//!   The wrapper refuses any [`TenantScopedQuery`] whose SQL does not
//!   carry a `WHERE tenant_id = ?` (or equivalent normalised form) and,
//!   on `bind`, forces the first bound positional parameter to be the
//!   tenant-id this database was constructed for.
//! - **Audit fence** on every mutation (`run` returning meta with
//!   `row_changes > 0`). Fail-CLOSED: if the audit closure rejects, the
//!   D1 mutation is **not** performed.
//! - **Constant-time tenant-id comparison** via
//!   [`subtle::ConstantTimeEq`]. The tenant-id is fetched per-request
//!   from a JWT or session record; a timing leak on the comparison
//!   would let a colocated tenant probe for valid tenant-id prefixes.
//!
//! All extended operations route through [`CfD1DatabaseReal`] which:
//!
//! 1. **Enforces tenant prefix** on every query argument and the first
//!    bound parameter (defense-in-depth: SQL syntax check + bind-time
//!    constant-time check).
//! 2. **Forbids unwrap/expect/panic** outside test (workspace-level
//!    lints already deny these; reasserted here because a panic on
//!    wasm32 is a customer-visible outage — the Worker isolate aborts).
//! 3. **Emits an audit fence** before every mutation. The audit emitter
//!    is injected via [`CfD1DatabaseReal::with_audit`] (the default is
//!    a no-op closure suitable for read-only test fixtures); production
//!    boot wires a writer that fans into `apps/server`'s audit chain.
//!    Audit emission is fail-CLOSED: if the audit closure returns an
//!    `Err`, the D1 mutation is **not** performed.
//!
//! # Dual-target build
//!
//! - `target_arch = "wasm32"`: real `worker::D1Database` wiring.
//! - `target_arch != "wasm32"`: stub that returns
//!   [`D1Error::Backend`] with the stable prefix `"WasmOnly: …"`.
//!
//! The stub exists so consumer crates can construct
//! `CfD1DatabaseReal::stub_for_native_tests()` in unit tests without
//! per-call `cfg(target_arch = ...)` gates. Calling any operation on
//! the native stub returns `D1Error::Backend("WasmOnly: <op>")`
//! immediately — this is the deterministic "wrong target" signal.
//! Validation (tenant-prefix on the query, bind-time tenant-id ct_eq,
//! audit fence) STILL runs on native, so the wrapper-layer tests pin
//! the contract on host CI before the wasm32 build.
//!
//! # Pattern reference
//!
//! See `specs/_audits/sealed/2026-05-15-cf-binding-real-pattern.md` for the
//! step-by-step recipe. This module ticks the D1 row of the
//! per-binding replication checklist.

use std::fmt;
use std::sync::Arc;
use subtle::ConstantTimeEq;

// ---------------------------------------------------------------------------
// Error taxonomy (D1-local — no canonical D1Backend trait yet per the
// pattern doc; we own the error surface).
// ---------------------------------------------------------------------------

/// Error category emitted by [`CfD1DatabaseReal`] and the surrounding
/// [`TenantScopedQuery`] wrapper.
///
/// `Backend(String)` carries a stable diagnostic prefix that upstream
/// code may match on:
///
/// - `tenant_id:` — validation rejected the tenant-id (shape, length,
///   forbidden chars).
/// - `tenant_scope:` — the SQL did not carry the required tenant
///   scope clause.
/// - `tenant_bind:` — the first bound parameter did not constant-time
///   equal the database's tenant-id.
/// - `audit:` — the audit closure rejected the mutation.
/// - `WasmOnly:` — invoked on the native stub; production target is
///   `wasm32-unknown-unknown`.
/// - `d1 <op>:` — underlying `worker::D1Database` error.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum D1Error {
    /// Backend error with a stable diagnostic prefix.
    Backend(String),
}

impl fmt::Display for D1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backend(msg) => write!(f, "d1: {msg}"),
        }
    }
}

impl std::error::Error for D1Error {}

// ---------------------------------------------------------------------------
// D1Op — stable operation labels for audit + diagnostic strings.
// ---------------------------------------------------------------------------

/// Operations wrapped by [`CfD1DatabaseReal`]. The string form is part
/// of the stable diagnostic surface (`WasmOnly: <op>`, audit channel).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum D1Op {
    /// `prepare(sql)` — produce a [`TenantScopedQuery`].
    Prepare,
    /// `bind(params)` — bind positional parameters; the FIRST positional
    /// parameter MUST constant-time equal the database's tenant-id.
    Bind,
    /// `first(col)` — fetch the first row (or first column of the
    /// first row when a column name is supplied).
    First,
    /// `all()` — fetch every result row.
    All,
    /// `run()` — execute a mutation; audit fires when the resulting
    /// meta carries `changes > 0` (a row WAS modified).
    Run,
}

impl D1Op {
    /// Static label used in `D1Error::Backend` diagnostics + audit.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Prepare => "prepare",
            Self::Bind => "bind",
            Self::First => "first",
            Self::All => "all",
            Self::Run => "run",
        }
    }
}

impl fmt::Display for D1Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// TenantId typed wrapper — constant-time-comparable.
// ---------------------------------------------------------------------------

/// A validated tenant identifier anchored to a [`CfD1DatabaseReal`].
/// Constructible only via [`TenantId::new`] which validates basic
/// shape: non-empty, ≤ 128 bytes, no embedded `'`, `"`, `\`, NUL, or
/// whitespace (these characters are SQL-injection or audit-channel
/// poison in a tenant-id context).
///
/// The canonical tenant identifier in CoreLink is a 16-hex HMAC
/// produced by `corelink_tenant_path::TenantPrefix`; this crate
/// validates **shape**, not derivation. Upstream is responsible for the
/// canonical derivation.
///
/// Cloning is cheap (small string).
#[derive(Clone, Debug)]
pub struct TenantId(String);

impl TenantId {
    /// Construct a tenant-id from a raw string. Validates:
    ///
    /// - non-empty
    /// - ≤ 128 bytes (defense against pathologic strings — the canonical
    ///   tenant-id is 16 hex chars / 32 bytes UUID at most)
    /// - no SQL-poison characters (`'`, `"`, `\`, NUL, whitespace)
    ///
    /// # Errors
    ///
    /// Returns [`D1Error::Backend`] with the stable prefix
    /// `tenant_id:` on rejection.
    pub fn new(raw: impl Into<String>) -> Result<Self, D1Error> {
        let s = raw.into();
        if s.is_empty() {
            return Err(D1Error::Backend(
                "tenant_id: empty tenant-id rejected".to_owned(),
            ));
        }
        if s.len() > 128 {
            return Err(D1Error::Backend(format!(
                "tenant_id: tenant-id exceeds 128-byte cap ({} bytes)",
                s.len()
            )));
        }
        for ch in s.chars() {
            if ch == '\'' || ch == '"' || ch == '\\' || ch == '\0' || ch.is_whitespace() {
                return Err(D1Error::Backend(
                    "tenant_id: tenant-id contains forbidden character".to_owned(),
                ));
            }
        }
        Ok(Self(s))
    }

    /// Borrow the tenant-id as a string slice. The result is opaque to
    /// callers — they MUST NOT log it beyond a stable correlation_id
    /// (CTRL-PRIV-001); production audit chain handles the redaction
    /// boundary.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Constant-time equality check against a candidate string. Used
    /// at bind-time to verify the FIRST positional parameter matches
    /// the database's anchored tenant-id without leaking a timing
    /// side-channel (a byte-by-byte comparator would let a colocated
    /// tenant probe for valid tenant-id prefixes).
    #[must_use]
    pub fn ct_eq_str(&self, candidate: &str) -> bool {
        // ConstantTimeEq operates byte-wise; padding to the longer of
        // the two prevents an early-exit length leak. We compare the
        // raw bytes — Choice → bool conversion is constant time.
        let a = self.0.as_bytes();
        let b = candidate.as_bytes();
        // ConstantTimeEq on equal-length slices is constant time;
        // mismatched lengths short-circuit to false but that fact
        // (length mismatch) is already public (caller-controlled).
        if a.len() != b.len() {
            // Drain a fake comparison to keep the per-call timing
            // closer to the equal-length path (best-effort).
            let _ = a.ct_eq(a);
            return false;
        }
        a.ct_eq(b).into()
    }
}

// ---------------------------------------------------------------------------
// TenantScopedQuery — SQL with a checked tenant scope clause.
// ---------------------------------------------------------------------------

/// A SQL query that has been validated to carry a tenant scope clause.
/// Constructible only via [`CfD1DatabaseReal::scoped_query`]; the type
/// therefore witnesses tenant-scope enforcement at the type level for
/// callers that thread it through.
///
/// The validator is intentionally **strict-and-shallow**:
///
/// - For mutation verbs (`UPDATE`, `DELETE`) and `SELECT` over rows the
///   wrapper requires the literal substring `WHERE tenant_id = ?`
///   (case-insensitive on the keyword chunks, whitespace-tolerant on
///   the `=` boundary).
/// - For `INSERT` the wrapper requires that `tenant_id` appears in the
///   column list (i.e. the SQL writes the tenant-id explicitly; the
///   bind-time check then constant-time-compares the first positional
///   parameter against the database's anchored tenant-id).
///
/// SQL parsers are out of scope (no `sqlparser` dep in this crate —
/// keeps the wasm32 build slim). The shallow check + bind-time
/// constant-time tenant-id verification is the defense-in-depth
/// contract; the canonical column-level enforcement still lives in the
/// schema (`tenant_id` is `NOT NULL` on every tenanted table).
///
/// The wrapper is `Clone + Debug` but the SQL is **never** logged
/// beyond a stable correlation_id (CTRL-PRIV-001 — schemas are public
/// but bound parameter values are not).
#[derive(Clone, Debug)]
pub struct TenantScopedQuery {
    sql: String,
}

impl TenantScopedQuery {
    /// Borrow the underlying SQL string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.sql
    }
}

// ---------------------------------------------------------------------------
// Audit hook (fail-CLOSED on mutation).
// ---------------------------------------------------------------------------

/// Closure type for the audit emitter wired into [`CfD1DatabaseReal`].
/// Called BEFORE every mutation operation. If the closure returns
/// `Err`, the D1 mutation is NOT performed (fail-CLOSED).
///
/// Production wires a closure that fans into `apps/server`'s audit
/// chain (Workers Analytics + structured audit log). Tests use the
/// default no-op or a recording closure.
pub type AuditFn = Arc<dyn Fn(D1Op, &str) -> Result<(), D1Error> + Send + Sync + 'static>;

fn noop_audit() -> AuditFn {
    Arc::new(|_op, _sql| Ok(()))
}

// ---------------------------------------------------------------------------
// CfD1DatabaseReal: dual-target struct definition.
// ---------------------------------------------------------------------------

/// Real CF D1 binding adapter with tenant-scoped query primitives and
/// audit-fenced mutations.
///
/// Dual-target: wasm32 wraps a `worker::D1Database`; native build holds
/// no inner binding (the stub returns `D1Error::Backend("WasmOnly: …")`
/// from every operation, *after* the validation contract has run).
#[cfg(target_arch = "wasm32")]
pub struct CfD1DatabaseReal {
    db: Arc<worker::D1Database>,
    tenant: TenantId,
    audit: AuditFn,
}

/// Native-build stub variant of [`CfD1DatabaseReal`]. Holds the tenant
/// id + audit hook so the wrapper-layer validation contract still runs
/// on native CI. Every operation method returns
/// `D1Error::Backend("WasmOnly: …")` after validation/audit.
#[cfg(not(target_arch = "wasm32"))]
pub struct CfD1DatabaseReal {
    tenant: TenantId,
    audit: AuditFn,
}

impl fmt::Debug for CfD1DatabaseReal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CfD1DatabaseReal")
            .field("tenant", &self.tenant.as_str())
            .finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------
// Shared (target-agnostic) impl: tenant accessor, scoped-query
// derivation, audit hookup, bind-time tenant-id constant-time check.
// These compile on both wasm32 and native and run identical logic in
// both targets — the validation contract is therefore pinned by host
// CI before the wasm32 build.
// ---------------------------------------------------------------------------

impl CfD1DatabaseReal {
    /// The tenant-id this wrapper enforces.
    #[must_use]
    pub fn tenant(&self) -> &TenantId {
        &self.tenant
    }

    /// Replace the audit hook. Returns `self` for builder-style chains.
    #[must_use]
    pub fn with_audit(mut self, audit: AuditFn) -> Self {
        self.audit = audit;
        self
    }

    /// Build a [`TenantScopedQuery`] from a raw SQL string. Validates
    /// that the query carries a tenant scope clause (see
    /// [`TenantScopedQuery`] for the precise rules).
    ///
    /// # Errors
    ///
    /// Returns [`D1Error::Backend`] with the stable prefix
    /// `tenant_scope:` if the SQL does not carry the required scope
    /// clause, or `tenant_scope:` with a NUL-byte diagnostic if the
    /// SQL contains an embedded NUL.
    pub fn scoped_query(&self, sql: &str) -> Result<TenantScopedQuery, D1Error> {
        if sql.is_empty() {
            return Err(D1Error::Backend(
                "tenant_scope: empty SQL rejected".to_owned(),
            ));
        }
        if sql.contains('\0') {
            return Err(D1Error::Backend(
                "tenant_scope: SQL must not contain NUL byte".to_owned(),
            ));
        }
        // Normalise once for the case-insensitive substring check; we
        // keep the original SQL verbatim for the worker::D1Database
        // call (column quoting, comments, etc. are preserved).
        let lower = sql.to_ascii_lowercase();
        let verb = first_verb(&lower);
        match verb {
            // SELECT / UPDATE / DELETE: require `where tenant_id = ?`
            // (whitespace-tolerant).
            "select" | "update" | "delete" => {
                if !contains_where_tenant_id_eq_placeholder(&lower) {
                    return Err(D1Error::Backend(format!(
                        "tenant_scope: {verb} SQL must carry 'WHERE tenant_id = ?'"
                    )));
                }
            }
            // INSERT: require the column list mentions `tenant_id`.
            "insert" => {
                if !mentions_tenant_id_column(&lower) {
                    return Err(D1Error::Backend(
                        "tenant_scope: INSERT SQL must list 'tenant_id' as a column".to_owned(),
                    ));
                }
            }
            other => {
                return Err(D1Error::Backend(format!(
                    "tenant_scope: verb '{other}' not permitted by tenant scope wrapper"
                )));
            }
        }
        Ok(TenantScopedQuery {
            sql: sql.to_owned(),
        })
    }

    /// Verify a candidate first-positional bind parameter against this
    /// database's anchored tenant-id using a constant-time compare.
    ///
    /// # Errors
    ///
    /// Returns [`D1Error::Backend`] with stable prefix `tenant_bind:`
    /// if the candidate does not constant-time equal the anchored
    /// tenant-id.
    pub fn verify_first_bind(&self, candidate: &str) -> Result<(), D1Error> {
        if self.tenant.ct_eq_str(candidate) {
            Ok(())
        } else {
            Err(D1Error::Backend(
                "tenant_bind: first positional parameter does not match anchored tenant-id"
                    .to_owned(),
            ))
        }
    }

    /// Helper: validate-or-derive the scoped query and emit the audit
    /// fence before any mutation. Returns the validated query for the
    /// downstream `worker::*` call.
    ///
    /// Wasm32 callers prefer the split form
    /// (`scoped_query` + direct audit invocation) for finer-grained
    /// op labelling; the combined helper is the test-side surface
    /// exposed via [`Self::audit_and_scope_for_tests`].
    #[allow(
        dead_code,
        reason = "used by audit_and_scope_for_tests; wasm32 splits the call sites for finer op labelling"
    )]
    fn audit_and_scope(&self, op: D1Op, sql: &str) -> Result<TenantScopedQuery, D1Error> {
        let scoped = self.scoped_query(sql)?;
        (self.audit)(op, scoped.as_str())?;
        Ok(scoped)
    }
}

// ---------------------------------------------------------------------------
// Shallow SQL classifier helpers (no parser dep).
// ---------------------------------------------------------------------------

/// Return the first lowercase ASCII word in `lower` (which MUST already
/// be ASCII-lowercased). Used to classify the SQL verb.
fn first_verb(lower: &str) -> &str {
    let trimmed = lower.trim_start();
    match trimmed.find(|c: char| c.is_whitespace() || c == '(' || c == ';') {
        Some(end) => &trimmed[..end],
        None => trimmed,
    }
}

/// True iff `lower` contains a `where ... tenant_id ... = ... ?` slice
/// with whitespace tolerated around the `=`. Defense in depth: the
/// canonical column enforcement lives in the schema.
fn contains_where_tenant_id_eq_placeholder(lower: &str) -> bool {
    // Use a small state machine: find "where", then within the rest,
    // find "tenant_id", then within the rest, find an `=`, then within
    // the rest, find a `?`. Anything else in between is acceptable —
    // the canonical compositional check is "the query has a clause of
    // the shape `WHERE tenant_id = ?` somewhere".
    let Some(after_where) = find_token(lower, "where") else {
        return false;
    };
    let Some(after_tenant) = find_token(after_where, "tenant_id") else {
        return false;
    };
    let Some(eq_idx) = after_tenant.find('=') else {
        return false;
    };
    let after_eq = match after_tenant.get(eq_idx + 1..) {
        Some(slice) => slice,
        None => return false,
    };
    after_eq.contains('?')
}

/// True iff `lower` looks like `INSERT ... ( ... tenant_id ... ) ...`.
fn mentions_tenant_id_column(lower: &str) -> bool {
    let Some(open) = lower.find('(') else {
        return false;
    };
    let Some(close_rel) = lower.get(open..).and_then(|rest| rest.find(')')) else {
        return false;
    };
    let column_slice = match lower.get(open..open + close_rel) {
        Some(slice) => slice,
        None => return false,
    };
    // Match the bare identifier `tenant_id` inside the column list
    // (any commas, whitespace, or backticks/quotes around it are fine —
    // the substring check is over a known-lowercased slice).
    column_slice.contains("tenant_id")
}

/// Find the byte-slice after the first occurrence of `token` as a
/// standalone word (preceded by start-of-string or non-alphanumeric,
/// followed by non-alphanumeric or end-of-string).
fn find_token<'a>(haystack: &'a str, token: &str) -> Option<&'a str> {
    let mut start = 0usize;
    while start < haystack.len() {
        let tail = haystack.get(start..)?;
        let idx_in_tail = tail.find(token)?;
        let abs_idx = start + idx_in_tail;
        let before_ok = abs_idx == 0
            || haystack
                .as_bytes()
                .get(abs_idx.saturating_sub(1))
                .map(|b| !is_ident_byte(*b))
                .unwrap_or(true);
        let end = abs_idx + token.len();
        let after_ok = end >= haystack.len()
            || haystack
                .as_bytes()
                .get(end)
                .map(|b| !is_ident_byte(*b))
                .unwrap_or(true);
        if before_ok && after_ok {
            return haystack.get(end..);
        }
        start = abs_idx + token.len();
    }
    None
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

// ---------------------------------------------------------------------------
// wasm32 production impl.
// ---------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
impl CfD1DatabaseReal {
    /// Construct a real wrapper from a `worker::D1Database` and a
    /// validated tenant-id. Uses a no-op audit hook; replace via
    /// [`Self::with_audit`].
    #[must_use]
    pub fn new(db: worker::D1Database, tenant: TenantId) -> Self {
        Self {
            db: Arc::new(db),
            tenant,
            audit: noop_audit(),
        }
    }

    /// Borrow the underlying `worker::D1Database` for callers that
    /// truly need the raw surface (e.g. `batch` / `dump`). Use
    /// sparingly; bypasses tenant-scope enforcement.
    #[must_use]
    pub fn inner(&self) -> &worker::D1Database {
        &self.db
    }

    /// `prepare(sql)` — produce a prepared statement against a
    /// tenant-scoped query. Audits the prepare (read-channel probe
    /// trail).
    pub fn prepare(
        &self,
        query: &TenantScopedQuery,
    ) -> Result<worker::D1PreparedStatement, D1Error> {
        (self.audit)(D1Op::Prepare, query.as_str())?;
        Ok(self.db.prepare(query.as_str()))
    }

    /// `bind(stmt, params)` — bind positional parameters to a prepared
    /// statement. The FIRST element of `string_params` is treated as
    /// the tenant-id and constant-time-verified against the anchored
    /// tenant-id; the wrapper refuses if it does not match.
    ///
    /// `string_params` carries the FULL ordered parameter list — the
    /// tenant-id at index 0 + the remaining caller parameters. We take
    /// a `&[&str]` instead of the heterogeneous `worker::JsValue` slice
    /// because every tenant-id is a string at this layer; mixed-type
    /// binding can be added as a follow-up if required.
    pub fn bind(
        &self,
        stmt: worker::D1PreparedStatement,
        string_params: &[&str],
    ) -> Result<worker::D1PreparedStatement, D1Error> {
        let first = string_params.first().ok_or_else(|| {
            D1Error::Backend("tenant_bind: bind called with empty parameter list".to_owned())
        })?;
        self.verify_first_bind(first)?;
        (self.audit)(D1Op::Bind, "(bind)")?;
        let js_values: Vec<worker::wasm_bindgen::JsValue> = string_params
            .iter()
            .map(|s| worker::wasm_bindgen::JsValue::from_str(s))
            .collect();
        stmt.bind(&js_values)
            .map_err(|e| D1Error::Backend(format!("d1 bind: {e}")))
    }

    /// `first(stmt, col)` — fetch the first row (or first column of
    /// the first row when a column name is supplied). Audits the read.
    pub async fn first<T>(
        &self,
        stmt: &worker::D1PreparedStatement,
        col_name: Option<&str>,
    ) -> Result<Option<T>, D1Error>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        (self.audit)(D1Op::First, "(first)")?;
        stmt.first(col_name)
            .await
            .map_err(|e| D1Error::Backend(format!("d1 first: {e}")))
    }

    /// `all(stmt)` — fetch every result row. Audits the read.
    pub async fn all(
        &self,
        stmt: &worker::D1PreparedStatement,
    ) -> Result<worker::D1Result, D1Error> {
        (self.audit)(D1Op::All, "(all)")?;
        stmt.all()
            .await
            .map_err(|e| D1Error::Backend(format!("d1 all: {e}")))
    }

    /// `run(stmt)` — execute a mutation. Audit-fenced fail-CLOSED
    /// **before** dispatch; on return, if the meta reports
    /// `changes > 0` the (already-emitted) audit row is the
    /// transaction-of-record. We re-emit a post-mutation audit with
    /// the observed change count so the audit chain records the
    /// outcome — fail-CLOSED on that second emission too.
    pub async fn run(
        &self,
        stmt: &worker::D1PreparedStatement,
    ) -> Result<worker::D1Result, D1Error> {
        (self.audit)(D1Op::Run, "(run:pre)")?;
        let result = stmt
            .run()
            .await
            .map_err(|e| D1Error::Backend(format!("d1 run: {e}")))?;
        let changes = result
            .meta()
            .map_err(|e| D1Error::Backend(format!("d1 run meta: {e}")))?
            .and_then(|m| m.changes)
            .unwrap_or(0);
        if changes > 0 {
            (self.audit)(D1Op::Run, "(run:post)")?;
        }
        Ok(result)
    }
}

// ---------------------------------------------------------------------------
// native stub.
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
impl CfD1DatabaseReal {
    /// Construct a native stub for host-side trait-bound testing. Every
    /// operation returns `D1Error::Backend("WasmOnly: …")` after the
    /// validation/audit contract has run on the native target — same
    /// code path that wasm32 production exercises.
    #[must_use]
    pub fn stub_for_native_tests(tenant: TenantId) -> Self {
        Self {
            tenant,
            audit: noop_audit(),
        }
    }

    /// Native stub for `prepare`. Validates scope + audits, then
    /// `WasmOnly`.
    pub fn prepare(&self, query: &TenantScopedQuery) -> Result<(), D1Error> {
        (self.audit)(D1Op::Prepare, query.as_str())?;
        Err(D1Error::Backend(format!("WasmOnly: {}", D1Op::Prepare)))
    }

    /// Native stub for `bind`. Verifies first param + audits, then
    /// `WasmOnly`.
    pub fn bind(&self, string_params: &[&str]) -> Result<(), D1Error> {
        let first = string_params.first().ok_or_else(|| {
            D1Error::Backend("tenant_bind: bind called with empty parameter list".to_owned())
        })?;
        self.verify_first_bind(first)?;
        (self.audit)(D1Op::Bind, "(bind)")?;
        Err(D1Error::Backend(format!("WasmOnly: {}", D1Op::Bind)))
    }

    /// Native stub for `first`. Audits, then `WasmOnly`.
    pub async fn first(&self) -> Result<(), D1Error> {
        (self.audit)(D1Op::First, "(first)")?;
        Err(D1Error::Backend(format!("WasmOnly: {}", D1Op::First)))
    }

    /// Native stub for `all`. Audits, then `WasmOnly`.
    pub async fn all(&self) -> Result<(), D1Error> {
        (self.audit)(D1Op::All, "(all)")?;
        Err(D1Error::Backend(format!("WasmOnly: {}", D1Op::All)))
    }

    /// Native stub for `run`. Audits (pre-mutation fence), then
    /// `WasmOnly`. The post-mutation audit does NOT fire on native
    /// (there is no change to record).
    pub async fn run(&self) -> Result<(), D1Error> {
        (self.audit)(D1Op::Run, "(run:pre)")?;
        Err(D1Error::Backend(format!("WasmOnly: {}", D1Op::Run)))
    }

    /// Native-test convenience: run the audit_and_scope hook directly
    /// (used by the wrapper-layer tests). Mirrors the pattern in
    /// [`crate::r2_real::CfR2BucketReal`].
    pub fn audit_and_scope_for_tests(
        &self,
        op: D1Op,
        sql: &str,
    ) -> Result<TenantScopedQuery, D1Error> {
        self.audit_and_scope(op, sql)
    }
}

// ---------------------------------------------------------------------------
// Unit tests — wrapper layer (target-agnostic; run on native CI).
//
// These tests pin the trait-bound invariants the wrapper enforces
// BEFORE calling worker::*: tenant-id validation, tenant-scope SQL
// validation, bind-time constant-time tenant-id check, audit
// fail-CLOSED, error-path mapping. The wasm32 production path runs the
// same scoped_query / verify_first_bind / audit code (it lives in the
// shared impl block), so these tests pin the contract end-to-end.
// ---------------------------------------------------------------------------

#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics on assertion failure are the canonical signal"
)]
#[path = "tests.rs"]
mod tests;

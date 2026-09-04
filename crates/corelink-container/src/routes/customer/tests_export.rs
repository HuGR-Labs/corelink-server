use super::*;

// ── Tenant bulk export (SEAM): POST /v1/customer/account/export ────────────

use crate::routes::customer_export::{
    BlobKind, ExportBlobRef, TenantExportError, TenantExportSource,
};
use base64::Engine as _;

/// In-memory export source keyed by tenant — the isolation boundary under
/// test (a request for A can NEVER surface B's blobs).
#[derive(Debug, Default)]
struct FakeExportSource {
    blobs: std::collections::HashMap<String, Vec<(ExportBlobRef, Vec<u8>)>>,
}

impl FakeExportSource {
    fn with_blob(mut self, tenant: &str, kind: BlobKind, digest: &str, bytes: &[u8]) -> Self {
        self.blobs.entry(tenant.to_owned()).or_default().push((
            ExportBlobRef {
                kind,
                digest: digest.to_owned(),
                size: bytes.len() as u64,
            },
            bytes.to_vec(),
        ));
        self
    }
}

impl TenantExportSource for FakeExportSource {
    fn metadata_records(&self, tenant: &str) -> Result<Vec<Value>, TenantExportError> {
        Ok(vec![json!({
            "kind": "rbac",
            "table": "team_member",
            "record": { "tenant_id": tenant, "role": "owner" },
        })])
    }
    fn blob_index(&self, tenant: &str) -> Result<Vec<ExportBlobRef>, TenantExportError> {
        Ok(self
            .blobs
            .get(tenant)
            .map(|v| v.iter().map(|(r, _)| r.clone()).collect())
            .unwrap_or_default())
    }
    fn fetch_blob(
        &self,
        tenant: &str,
        kind: BlobKind,
        digest: &str,
    ) -> Result<Option<Vec<u8>>, TenantExportError> {
        Ok(self.blobs.get(tenant).and_then(|v| {
            v.iter()
                .find(|(r, _)| r.kind == kind && r.digest == digest)
                .map(|(_, b)| b.clone())
        }))
    }
    fn record_export_audit(
        &self,
        _tenant: &str,
        _blob_count: usize,
        _metadata_count: usize,
    ) -> Result<(), TenantExportError> {
        Ok(())
    }
}

fn export_state(source: Option<Arc<dyn TenantExportSource>>) -> CustomerRouteState {
    let (mut state, _) = fixture();
    state.export = source;
    state
}

#[tokio::test]
async fn export_streams_bundle_with_blobs_and_records() {
    let src = FakeExportSource::default()
        .with_blob("tenant-a", BlobKind::Cas, "cafe", b"blob-A")
        .with_blob("tenant-a", BlobKind::Ac, "beef", b"ac-A");
    let app = router(export_state(Some(Arc::new(src))));
    let req = Request::builder()
        .uri("/v1/customer/account/export")
        .method("POST")
        .header("x-corelink-scope", "read-write billing")
        .header("x-corelink-role", "owner")
        .header("x-corelink-tenant-id", "tenant-a")
        .header("x-corelink-token-prefix", "clerk")
        .header("x-corelink-role", "owner")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    let lines: Vec<Value> = text
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines[0]["kind"], "header");
    let b64 = base64::engine::general_purpose::STANDARD;
    let cas = lines
        .iter()
        .find(|l| l["kind"] == "blob" && l["blob_kind"] == "cas")
        .expect("cas blob line");
    assert_eq!(
        b64.decode(cas["bytes"].as_str().unwrap()).unwrap(),
        b"blob-A"
    );
    let ac = lines
        .iter()
        .find(|l| l["kind"] == "blob" && l["blob_kind"] == "ac")
        .expect("ac blob line");
    assert_eq!(b64.decode(ac["bytes"].as_str().unwrap()).unwrap(), b"ac-A");
    assert!(lines.iter().any(|l| l["kind"] == "rbac"));
    assert_eq!(lines.last().unwrap()["kind"], "manifest");
}

#[tokio::test]
async fn export_cross_tenant_isolation() {
    let src = FakeExportSource::default()
        .with_blob("tenant-a", BlobKind::Cas, "aaaa", b"A-secret")
        .with_blob("tenant-b", BlobKind::Cas, "bbbb", b"B-secret");
    let app = router(export_state(Some(Arc::new(src))));
    let req = Request::builder()
        .uri("/v1/customer/account/export")
        .method("POST")
        .header("x-corelink-scope", "read-write billing")
        .header("x-corelink-role", "owner")
        .header("x-corelink-tenant-id", "tenant-a")
        .header("x-corelink-token-prefix", "clerk")
        .header("x-corelink-role", "owner")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    let b64 = base64::engine::general_purpose::STANDARD;
    assert!(text.contains(&b64.encode(b"A-secret")));
    assert!(
        !text.contains(&b64.encode(b"B-secret")),
        "must not leak tenant-b bytes"
    );
    assert!(!text.contains("bbbb"), "must not leak tenant-b digest");
}

#[tokio::test]
async fn export_unauthenticated_tenant_401() {
    let app = router(export_state(Some(Arc::new(FakeExportSource::default()))));
    // No x-corelink-tenant-id header → fail-CLOSED 401.
    let req = Request::builder()
        .uri("/v1/customer/account/export")
        .method("POST")
        .header("x-corelink-scope", "read-write")
        .header("x-corelink-role", "owner")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn export_readonly_scope_forbidden() {
    let app = router(export_state(Some(Arc::new(FakeExportSource::default()))));
    // A read-only cache token must not export team PII / DPA / audit / blobs.
    let req = Request::builder()
        .uri("/v1/customer/account/export")
        .method("POST")
        .header("x-corelink-scope", "read-only")
        .header("x-corelink-tenant-id", "tenant-a")
        .header("x-corelink-token-prefix", "clpat_ro")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn export_unwired_source_fails_closed_503() {
    let app = router(export_state(None));
    let req = Request::builder()
        .uri("/v1/customer/account/export")
        .method("POST")
        .header("x-corelink-scope", "read-write billing")
        .header("x-corelink-role", "owner")
        .header("x-corelink-tenant-id", "tenant-a")
        .header("x-corelink-token-prefix", "clerk")
        .header("x-corelink-role", "owner")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

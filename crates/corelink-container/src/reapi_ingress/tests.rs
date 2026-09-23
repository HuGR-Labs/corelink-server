//! Behavior coverage for the authenticated ingress boundary.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: panics surface as failures by design"
)]
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

#[derive(Debug)]
struct TestAuth(Result<(String, bool), AuthenticationFailure>);

#[async_trait]
impl IngressAuthenticator for TestAuth {
    async fn authenticate(&self, _bearer: &str) -> Result<(String, bool), AuthenticationFailure> {
        self.0.clone()
    }
}

#[derive(Debug)]
struct TestAdmission {
    result: Result<(), AdmissionFailure>,
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl IngressAdmission for TestAdmission {
    async fn admit(&self, _tenant_id: &str) -> Result<AdmissionLease, AdmissionFailure> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.result.map(|()| AdmissionLease::test_only())
    }
}

#[derive(Debug)]
struct TestCapResolver;

#[async_trait]
impl crate::oci_cap::TenantCapResolver for TestCapResolver {
    async fn resolve_storage_cap(&self, _tenant_id: &str) -> Option<i64> {
        Some(0)
    }
}

#[test]
fn canonical_sha256_and_resource_names_are_shared_and_strict() {
    let digest = sha256_digest(b"corelink");
    assert_eq!(digest.len(), 64);
    assert!(validate_digest(&digest, 8).is_ok());
    assert!(validate_digest(&digest.to_ascii_uppercase(), 8).is_err());
    assert!(validate_digest(&digest, -1).is_err());
    let read =
        validate_blob_resource_name(&format!("tenant-a/blobs/{digest}/8"), "tenant-a").unwrap();
    assert_eq!(read.instance_name(), "tenant-a");
    assert_eq!(read.hash(), digest);
    assert_eq!(read.size_bytes(), 8);
    assert_eq!(read.upload_id(), None);
    let write = validate_blob_resource_name(
        &format!("tenant-a/uploads/00000000-0000-0000-0000-000000000001/blobs/{digest}/8"),
        "tenant-a",
    )
    .unwrap();
    assert_eq!(
        write.upload_id(),
        Some("00000000-0000-0000-0000-000000000001")
    );
    assert!(validate_blob_resource_name("_public/blobs/abcd/4", "tenant-a").is_err());
    assert!(validate_blob_resource_name("tenant-a/blobs/abcd/4/extra", "tenant-a").is_err());
    assert!(
        validate_blob_resource_name("tenant-a/uploads/not-a-uuid/blobs/abcd/4", "tenant-a")
            .is_err()
    );
}

#[tokio::test]
async fn denied_write_never_reaches_any_shared_storage_handler() {
    let touched = Arc::new(AtomicUsize::new(0));
    let ingress = ingress_for_test(Ok(("tenant-a".into(), false)), Ok(()), touched.clone());
    let mut metadata = tonic::metadata::MetadataMap::new();
    metadata.insert("authorization", "Bearer placeholder".parse().unwrap());

    let error = ingress
        .authorize(&metadata, "tenant-a", Access::Write)
        .await
        .unwrap_err();
    assert_eq!(error.code(), Code::PermissionDenied);
    assert_eq!(touched.load(Ordering::SeqCst), 0);

    let read_only = ingress_for_test(Ok(("tenant-a".into(), false)), Ok(()), touched.clone());
    let admitted = read_only
        .authorize(&metadata, "tenant-a", Access::Read)
        .await
        .unwrap();
    assert_eq!(
        admitted
            .cas_write(&"a".repeat(64), 0, vec![])
            .await
            .unwrap_err()
            .code(),
        Code::PermissionDenied
    );
    assert_eq!(
        admitted
            .ac_update(&"a".repeat(64), 0, vec![])
            .await
            .unwrap_err()
            .code(),
        Code::PermissionDenied
    );
    assert_eq!(touched.load(Ordering::SeqCst), 0);
    assert!(!format!("{admitted:?}").contains("placeholder"));
}

#[tokio::test]
async fn auth_and_admission_failures_map_to_protocol_statuses() {
    let touched = Arc::new(AtomicUsize::new(0));
    let mut metadata = tonic::metadata::MetadataMap::new();
    let absent = ingress_for_test(Ok(("tenant-a".into(), true)), Ok(()), touched.clone());
    assert_eq!(
        absent
            .authorize(&metadata, "tenant-a", Access::Read)
            .await
            .unwrap_err()
            .code(),
        Code::Unauthenticated
    );
    metadata.insert("authorization", "Basic placeholder".parse().unwrap());
    assert_eq!(
        absent
            .authorize(&metadata, "tenant-a", Access::Read)
            .await
            .unwrap_err()
            .code(),
        Code::Unauthenticated
    );
    metadata.insert("authorization", "Bearer placeholder".parse().unwrap());

    let invalid = ingress_for_test(Err(AuthenticationFailure::Invalid), Ok(()), touched.clone());
    assert_eq!(
        invalid
            .authorize(&metadata, "tenant-a", Access::Read)
            .await
            .unwrap_err()
            .code(),
        Code::Unauthenticated
    );
    let backend = ingress_for_test(
        Err(AuthenticationFailure::Unavailable),
        Ok(()),
        touched.clone(),
    );
    assert_eq!(
        backend
            .authorize(&metadata, "tenant-a", Access::Read)
            .await
            .unwrap_err()
            .code(),
        Code::Unavailable
    );
    let exhausted = ingress_for_test(
        Ok(("tenant-a".into(), true)),
        Err(AdmissionFailure::Exhausted),
        touched.clone(),
    );
    assert_eq!(
        exhausted
            .authorize(&metadata, "tenant-a", Access::Read)
            .await
            .unwrap_err()
            .code(),
        Code::ResourceExhausted
    );
    let unavailable = ingress_for_test(
        Ok(("tenant-a".into(), true)),
        Err(AdmissionFailure::Unavailable),
        touched.clone(),
    );
    assert_eq!(
        unavailable
            .authorize(&metadata, "tenant-a", Access::Read)
            .await
            .unwrap_err()
            .code(),
        Code::Unavailable
    );
    assert_eq!(touched.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn reserved_and_mismatched_instances_are_rejected_before_admission() {
    let mut metadata = tonic::metadata::MetadataMap::new();
    metadata.insert("authorization", "Bearer placeholder".parse().unwrap());
    let touched = Arc::new(AtomicUsize::new(0));
    let ingress = ingress_for_test(Ok(("tenant-a".into(), true)), Ok(()), touched.clone());
    assert_eq!(
        ingress
            .authorize(&metadata, "_public", Access::Read)
            .await
            .unwrap_err()
            .code(),
        Code::PermissionDenied
    );
    assert_eq!(
        ingress
            .authorize(&metadata, "tenant-b", Access::Read)
            .await
            .unwrap_err()
            .code(),
        Code::PermissionDenied
    );
    assert_eq!(ingress.admission_calls.load(Ordering::SeqCst), 0);
    assert_eq!(touched.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn malformed_authorization_is_rejected_without_storage_access() {
    let touched = Arc::new(AtomicUsize::new(0));
    let ingress = ingress_for_test(Ok(("tenant-a".into(), true)), Ok(()), touched.clone());
    let mut metadata = tonic::metadata::MetadataMap::new();
    metadata.insert("authorization", "Bearer bad header".parse().unwrap());
    assert_eq!(
        ingress
            .authorize(&metadata, "tenant-a", Access::Read)
            .await
            .unwrap_err()
            .code(),
        Code::Unauthenticated
    );
    assert_eq!(ingress.admission_calls.load(Ordering::SeqCst), 0);
    metadata.insert("authorization", "Bearer placeholder".parse().unwrap());
    metadata.append("authorization", "Bearer placeholder".parse().unwrap());
    assert_eq!(
        ingress
            .authorize(&metadata, "tenant-a", Access::Read)
            .await
            .unwrap_err()
            .code(),
        Code::Unauthenticated
    );
    assert_eq!(touched.load(Ordering::SeqCst), 0);
}

fn ingress_for_test(
    auth: Result<(String, bool), AuthenticationFailure>,
    admission: Result<(), AdmissionFailure>,
    storage_calls: Arc<AtomicUsize>,
) -> ReapiIngress {
    use corelink_handler_ac::{
        AcLookupRequest, AcLookupResponse, AcUpdateHandler, AcUpdateRequest, AcUpdateResponse,
    };
    use corelink_handler_cas::{
        CasReadRequest, CasReadResponse, CasWriteHandler, CasWriteRequest, CasWriteResponse,
    };

    #[derive(Debug)]
    struct CasRead(Arc<AtomicUsize>);
    impl CasReadHandler for CasRead {
        fn read(
            &self,
            _request: CasReadRequest,
        ) -> Result<CasReadResponse, corelink_handler_cas::CasHandlerError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            panic!("denied ingress reached CAS read")
        }
    }
    #[derive(Debug)]
    struct CasWrite(Arc<AtomicUsize>);
    impl CasWriteHandler for CasWrite {
        fn write(
            &self,
            _request: CasWriteRequest,
        ) -> Result<CasWriteResponse, corelink_handler_cas::CasHandlerError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            panic!("denied ingress reached CAS write")
        }
    }
    #[derive(Debug)]
    struct AcLookup(Arc<AtomicUsize>);
    impl AcLookupHandler for AcLookup {
        fn lookup(
            &self,
            _request: AcLookupRequest,
        ) -> Result<AcLookupResponse, corelink_handler_ac::AcHandlerError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            panic!("denied ingress reached AC lookup")
        }
    }
    #[derive(Debug)]
    struct AcUpdate(Arc<AtomicUsize>);
    impl AcUpdateHandler for AcUpdate {
        fn update(
            &self,
            _request: AcUpdateRequest,
        ) -> Result<AcUpdateResponse, corelink_handler_ac::AcHandlerError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            panic!("denied ingress reached AC update")
        }
    }

    let admission_calls = Arc::new(AtomicUsize::new(0));
    ReapiIngress {
        authenticator: Arc::new(TestAuth(auth)),
        admission: Arc::new(TestAdmission {
            result: admission,
            calls: admission_calls.clone(),
        }),
        cas_read: Arc::new(CasRead(storage_calls.clone())),
        cas_write: Arc::new(CasWrite(storage_calls.clone())),
        ac_lookup: Arc::new(AcLookup(storage_calls.clone())),
        ac_update: Arc::new(AcUpdate(storage_calls)),
        cap_resolver: Arc::new(TestCapResolver),
        admission_calls,
    }
}

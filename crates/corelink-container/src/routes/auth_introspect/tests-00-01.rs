    /// Build a `runner_repo_allowlist` `SELECT 1` result row (the column name is
    /// irrelevant — the decode keys off row PRESENCE, not a value).
    fn allowlist_hit_rows() -> Vec<crate::storage::d1_http::D1Row> {
        let mut row = serde_json::Map::new();
        row.insert("1".to_owned(), serde_json::json!(1));
        vec![row]
    }

    #[test]
    fn decode_resolved_installation_tenant_mapped_yields_some() {
        // A `tenant_gh_installation_map` row carrying the installation's tenant →
        // Some(tenant_id): the resolution that backs the fabric-plane runner mint.
        let mut row = serde_json::Map::new();
        row.insert(
            "tenant_id".to_owned(),
            serde_json::json!("22222222-2222-4222-8222-222222222222"),
        );
        assert_eq!(
            decode_resolved_installation_tenant(&[row]),
            Some("22222222-2222-4222-8222-222222222222".to_owned()),
            "a mapped installation row must resolve to its tenant"
        );
    }

    #[test]
    fn decode_resolved_installation_tenant_unmapped_yields_none() {
        // Empty result set (no row until provisioning) → None → the caller
        // returns 404 installation_not_mapped (never auto-provisions).
        assert_eq!(
            decode_resolved_installation_tenant(&[]),
            None,
            "an unmapped installation must yield None (→ 404 installation_not_mapped)"
        );
    }

    #[test]
    fn decode_resolved_installation_tenant_malformed_row_yields_none() {
        // A row whose tenant_id is missing / non-string / empty must NOT resolve
        // to a wrong/blank tenant — it falls to None (clean 404), never a bad
        // isolation boundary (mirrors decode_resolved_tenant).
        let mut missing = serde_json::Map::new();
        missing.insert("other".to_owned(), serde_json::json!("x"));
        assert_eq!(decode_resolved_installation_tenant(&[missing]), None);

        let mut non_str = serde_json::Map::new();
        non_str.insert("tenant_id".to_owned(), serde_json::json!(42));
        assert_eq!(decode_resolved_installation_tenant(&[non_str]), None);

        let mut empty = serde_json::Map::new();
        empty.insert("tenant_id".to_owned(), serde_json::json!(""));
        assert_eq!(decode_resolved_installation_tenant(&[empty]), None);
    }

    #[tokio::test]
    async fn resolve_tenant_for_installation_errors_on_d1_fault() {
        // The I/O wrapper surfaces a D1 fault as Err (→ caller 503), mirroring
        // resolve_tenant_for_org's fail-CLOSED contract.
        let d1 = unreachable_d1();
        let err = resolve_tenant_for_installation(&d1, "12345678").await;
        assert!(
            err.is_err(),
            "a D1 fault must surface as Err (fail-CLOSED → 503)"
        );
    }

    #[test]
    fn decode_repo_on_allowlist_hit_yields_true() {
        // A present `SELECT 1` row means the repo is explicitly allowed.
        assert!(
            decode_repo_on_allowlist(&allowlist_hit_rows()),
            "an allowlist hit (row present) must yield true"
        );
    }

    #[test]
    fn decode_repo_on_allowlist_miss_yields_false() {
        // No row → the repo is NOT allowlisted for the tenant → deny.
        assert!(
            !decode_repo_on_allowlist(&[]),
            "an allowlist miss (no row) must yield false"
        );
    }

    #[tokio::test]
    async fn repo_on_tenant_allowlist_errors_on_d1_fault() {
        // The I/O wrapper surfaces a D1 fault as Err so the caller denies the
        // mint fail-CLOSED (never grants on a backend fault).
        let d1 = unreachable_d1();
        let err = repo_on_tenant_allowlist(
            &d1,
            "22222222-2222-4222-8222-222222222222",
            "octocat/hello-world",
        )
        .await;
        assert!(
            err.is_err(),
            "a D1 fault on the allowlist read must surface as Err (fail-CLOSED)"
        );
    }

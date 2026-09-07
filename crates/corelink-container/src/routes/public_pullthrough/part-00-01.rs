impl UpstreamManifestResolver {
    /// Promote the FULL TRANSITIVE CLOSURE of an allowlisted-root DIGEST into the
    /// shared `_public` namespace, then serve the just-promoted manifest. Returns
    /// `None` on ANY failure (fail-open: the caller falls through to the M1
    /// per-tenant path) — and, critically, the root index/manifest is stored LAST
    /// (only after every child + blob verified + stored), so a mid-closure
    /// failure never leaves a `_public` root that a later existence read would
    /// serve as a COMPLETE closure. Orphaned children/blobs left behind are
    /// individually digest-verified + content-addressed (harmless; a re-promote
    /// is idempotent).
    ///
    /// Every descriptor is digest-verified against fetched bytes BEFORE its
    /// `_public` write (fail-CLOSED). `repo` is gated through the SAME
    /// origin-escape guard the mirror uses before any upstream fetch.
    async fn promote_public_closure(
        &self,
        repo: &str,
        root_digest: &str,
    ) -> Option<ResolvedManifest> {
        // Anti-bloat ceiling (B4): refuse once this instance has promoted enough
        // into `_public`. Fail-open — the tenant still gets a per-tenant resolve.
        if !self.public_under_ceiling() {
            return None;
        }
        if validate_repository(repo).is_err() {
            return None;
        }
        // Fetch + verify the ROOT by its immutable digest.
        let parsed_root = OciDigest::parse(root_digest).ok()?;
        let fetched = self
            .manifest_fetcher
            .fetch_manifest(repo, root_digest)
            .await
            .ok()?;
        if fetched.bytes.len() > MAX_PULLTHROUGH_MANIFEST_BYTES {
            return None;
        }
        // Fail-CLOSED: the fetched bytes MUST hash to the allowlisted digest.
        parsed_root.verify_against_bytes(&fetched.bytes).ok()?;
        let json = serde_json::from_slice::<serde_json::Value>(&fetched.bytes).ok()?;
        let content_type = manifest_media_type(fetched.content_type.as_deref(), &json);

        if let Some(children) = index_child_digests(&json) {
            // INDEX: promote each per-arch child (its config + layers + the child
            // manifest) FIRST; only if ALL succeed store the index bytes LAST.
            for child in &children {
                if !self.promote_public_child_manifest(repo, child).await {
                    return None;
                }
            }
            if self
                .moat
                .put(
                    PUBLIC_NAMESPACE,
                    root_digest,
                    fetched.bytes.clone(),
                    PUBLIC_CAP,
                )
                .await
                .is_err()
            {
                return None;
            }
            self.note_public_bytes(fetched.bytes.len());
        } else if let Some(blob_digests) = image_blob_digests(&json) {
            // IMAGE manifest: promote config + layers FIRST, store the manifest
            // LAST (so a partial blob failure never leaves a served-as-complete
            // manifest in `_public`).
            for d in &blob_digests {
                if !self
                    .persist_blob(PUBLIC_NAMESPACE, repo, d, PUBLIC_CAP)
                    .await
                {
                    return None;
                }
            }
            if self
                .moat
                .put(
                    PUBLIC_NAMESPACE,
                    root_digest,
                    fetched.bytes.clone(),
                    PUBLIC_CAP,
                )
                .await
                .is_err()
            {
                return None;
            }
            self.note_public_bytes(fetched.bytes.len());
        } else {
            // Neither an index nor an image manifest ⇒ nothing to promote.
            return None;
        }

        Some(ResolvedManifest {
            bytes: Bytes::from(fetched.bytes),
            content_type,
            digest: root_digest.to_owned(),
        })
    }

    /// Promote ONE per-arch child (an image manifest) of an index into `_public`:
    /// fetch it BY digest, digest-verify (fail-CLOSED), promote its config +
    /// layer blobs, then store the child manifest bytes LAST. `false` on any
    /// failure (the caller aborts the whole closure promote, fail-open).
    async fn promote_public_child_manifest(&self, repo: &str, child_digest: &str) -> bool {
        let Ok(parsed) = OciDigest::parse(child_digest) else {
            return false;
        };
        let canonical = parsed.to_wire();
        let Ok(fetched) = self.manifest_fetcher.fetch_manifest(repo, &canonical).await else {
            return false;
        };
        if fetched.bytes.len() > MAX_PULLTHROUGH_MANIFEST_BYTES {
            return false;
        }
        if parsed.verify_against_bytes(&fetched.bytes).is_err() {
            return false;
        }
        let Ok(json) = serde_json::from_slice::<serde_json::Value>(&fetched.bytes) else {
            return false;
        };
        // A child of an index is an IMAGE manifest (config + layers).
        let Some(blob_digests) = image_blob_digests(&json) else {
            return false;
        };
        for d in &blob_digests {
            if !self
                .persist_blob(PUBLIC_NAMESPACE, repo, d, PUBLIC_CAP)
                .await
            {
                return false;
            }
        }
        let len = fetched.bytes.len();
        let ok = self
            .moat
            .put(PUBLIC_NAMESPACE, &canonical, fetched.bytes, PUBLIC_CAP)
            .await
            .is_ok();
        if ok {
            self.note_public_bytes(len);
        }
        ok
    }
}

/// Upper bound on distinct single-flight keys held at once (bounded memory under
/// reference churn; ~tens of bytes per entry).
const INFLIGHT_MAP_CAP: usize = 50_000;

/// Return the current wall-clock time in ms since the Unix epoch (saturating).
fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

/// Choose the media type to serve: prefer the upstream `Content-Type`, else the
/// manifest body's `mediaType`, else the canonical OCI v1 default.
fn manifest_media_type(fetched_ct: Option<&str>, body: &serde_json::Value) -> String {
    if let Some(ct) = fetched_ct {
        let trimmed = ct.trim();
        if !trimmed.is_empty() {
            return trimmed.to_owned();
        }
    }
    if let Some(mt) = body.get("mediaType").and_then(serde_json::Value::as_str) {
        if !mt.is_empty() {
            return mt.to_owned();
        }
    }
    DEFAULT_MANIFEST_MEDIA_TYPE.to_owned()
}

/// Collect a JSON image manifest's `config.digest` + every `layers[].digest`,
/// or `None` when the doc is NOT an image manifest (an index/list carries
/// `manifests`, not `config`+`layers`). A returned `Some(vec)` means "this is an
/// image; eagerly resolve these blobs".
fn image_blob_digests(body: &serde_json::Value) -> Option<Vec<String>> {
    let config = body.get("config")?;
    let layers = body.get("layers")?.as_array()?;
    let mut digests = Vec::with_capacity(layers.len() + 1);
    let config_digest = config.get("digest").and_then(serde_json::Value::as_str)?;
    digests.push(config_digest.to_owned());
    for layer in layers {
        let d = layer.get("digest").and_then(serde_json::Value::as_str)?;
        digests.push(d.to_owned());
    }
    Some(digests)
}

/// Collect an OCI index / manifest-list's per-arch child manifest digests,
/// keeping ONLY `linux` children (`platform.os == "linux"`), or `None` when the
/// doc is NOT an index (an image manifest carries `config`+`layers`, not
/// `manifests`). A returned `Some(vec)` means "this is an index; recurse-promote
/// these children".
///
/// The filter is `os == "linux"`, NOT merely "not `unknown`": besides the
/// `unknown/unknown` attestation entries (buildkit SBOM / provenance), a real
/// base image's index also carries non-linux runnable entries (e.g. `golang`'s
/// `windows/amd64` child). CoreLink runners are linux, so promoting a windows
/// image manifest + its layers into `_public` is fetch/storage we would never
/// serve — skip everything but linux. A child kept this way must still carry a
/// `digest`; a linux child missing one is a malformed index ⇒ `None`
/// (fail-closed, nothing promoted).
fn index_child_digests(body: &serde_json::Value) -> Option<Vec<String>> {
    let manifests = body.get("manifests")?.as_array()?;
    let mut out = Vec::with_capacity(manifests.len());
    for m in manifests {
        let os = m
            .get("platform")
            .and_then(|p| p.get("os"))
            .and_then(serde_json::Value::as_str);
        if os != Some("linux") {
            // Skip windows / unknown-attestation / any non-linux child — never
            // served on a linux runner, so never worth promoting to `_public`.
            continue;
        }
        let d = m.get("digest").and_then(serde_json::Value::as_str)?;
        out.push(d.to_owned());
    }
    Some(out)
}

#[async_trait]
impl ManifestResolver for UpstreamManifestResolver {
    async fn resolve_on_miss(
        &self,
        tenant: &TenantId,
        repo: &str,
        reference: &str,
    ) -> PortResult<Option<ResolvedManifest>> {
        // 1. Rate-limit the per-tenant upstream-fetch rate. Over-limit ⇒
        //    fail-open (Ok(None)); the tenant's build falls back to docker.io.
        if !self.rate_limit_ok(tenant) {
            return Ok(None);
        }

        // 2. Single-flight on (tenant, repo, reference) so concurrent misses of
        //    the SAME reference coalesce into ONE upstream fetch.
        let sf_key = format!("{}|{repo}|{reference}", tenant.to_canonical_text());
        let lock = self.inflight_lock(&sf_key).await;
        let _guard = lock.lock().await;
        // A peer flight may have populated KV while we waited — serve that
        // instead of fetching again.
        if let Some(rm) = self.serve_from_kv(tenant, repo, reference).await {
            return Ok(Some(rm));
        }

        // 2.5 CROSS-TENANT `_public` decision (M2), BEFORE the per-tenant path.
        //     Only a DIGEST reference is `_public`-eligible — a TAG is mutable
        //     (first-writer poisoning) so it ALWAYS takes the M1 per-tenant path
        //     and never reads or writes `_public`.
        if self.dedup {
            if let Ok(parsed_ref) = OciDigest::parse(reference) {
                let canonical_ref = parsed_ref.to_wire();
                // `_public` READ (existence): a cross-tenant HIT needs NO upstream
                // call — serve the shared copy (byte-identical, content-addressed;
                // revocation still filters inside `MoatCache::get`). This serves a
                // transitively-promoted child that is NOT individually allowlisted.
                if let Some(rm) = self.serve_from_public(&canonical_ref).await {
                    return Ok(Some(rm));
                }
                // `_public` WRITE: only an allowlisted ROOT digest promotes its
                // full transitive closure into `_public`. Fail-open — a failed
                // promote falls through to the M1 per-tenant path below.
                if self.allowlist.is_allowlisted(&canonical_ref) {
                    if let Some(rm) = self.promote_public_closure(repo, &canonical_ref).await {
                        return Ok(Some(rm));
                    }
                }
            }
        }

        // 3. Gate the repo through the SAME origin-escape guard the mirror uses
        //    (before any upstream fetch).
        if validate_repository(repo).is_err() {
            return Ok(None);
        }

        // 4. Fetch the manifest from the FIXED upstream (multi-Accept). The
        //    fetcher caps the body; over-size ⇒ Err ⇒ fail-open.
        let fetched = match self.manifest_fetcher.fetch_manifest(repo, reference).await {
            Ok(f) => f,
            Err(_) => return Ok(None),
        };
        if fetched.bytes.len() > MAX_PULLTHROUGH_MANIFEST_BYTES {
            return Ok(None);
        }

        // 5. Verify / compute the digest. A `sha256:`/`sha512:` reference MUST
        //    hash to the fetched bytes (never serve unverified); a tag reference
        //    computes the digest.
        let digest_wire = match OciDigest::parse(reference) {
            Ok(declared) => {
                if declared.verify_against_bytes(&fetched.bytes).is_err() {
                    // Digest-lie (or an unsupported-algo reference) ⇒ fail-open,
                    // nothing stored.
                    return Ok(None);
                }
                declared.to_wire()
            }
            Err(_) => match OciDigest::compute(OciDigestAlgo::Sha256, &fetched.bytes) {
                Ok(d) => d.to_wire(),
                Err(_) => return Ok(None),
            },
        };

        // Defensive cross-check: the resolver always TRUSTS its own recomputed
        // digest (never the upstream's claimed one), but a disagreement with the
        // upstream `Docker-Content-Digest` is worth an observability breadcrumb.
        if let Some(upstream_digest) = fetched.docker_content_digest.as_deref() {
            if upstream_digest != digest_wire {
                tracing::debug!(
                    repo,
                    reference,
                    upstream = upstream_digest,
                    computed = %digest_wire,
                    "oci upstream-on-miss: Docker-Content-Digest disagrees with computed digest (trusting computed)"
                );
            }
        }

        // 6. Parse the doc + decide image vs index.
        let Ok(json) = serde_json::from_slice::<serde_json::Value>(&fetched.bytes) else {
            return Ok(None);
        };
        let content_type = manifest_media_type(fetched.content_type.as_deref(), &json);
        let tenant_ns = tenant.to_canonical_text();

        // For an IMAGE manifest, eagerly fetch + verify + persist its config +
        // layer blobs into the tenant's OWN moat FIRST — so a partial failure
        // never leaves a served manifest whose by-digest blob GETs (which carry
        // no repo) would 404. (Ordering note: the design lists manifest-then-
        // blobs; persisting blobs first strictly strengthens the fail-open
        // guarantee without changing observable success behavior.) An INDEX is
        // stored as-is with NO recursion — buildkit GETs the per-arch manifest
        // by digest as its own on-miss (M1 does not walk the index).
        if let Some(digests) = image_blob_digests(&json) {
            let cap = self.resolve_cap(&tenant_ns).await;
            for d in &digests {
                if !self.persist_blob(&tenant_ns, repo, d, cap).await {
                    return Ok(None);
                }
            }
        }

        // 7. Persist the manifest into the per-tenant KV. For a TAG, also write
        //    the by-digest slot so a later by-digest GET hits; write the
        //    companion content-type slot for each stored reference.
        let body = Bytes::from(fetched.bytes);
        if !self
            .persist_manifest(tenant, repo, reference, &digest_wire, &content_type, &body)
            .await
        {
            return Ok(None);
        }

        // 8. Serve.
        Ok(Some(ResolvedManifest {
            bytes: body,
            content_type,
            digest: digest_wire,
        }))
    }
}

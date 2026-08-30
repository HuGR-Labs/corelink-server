LA1 | CONFIRMED | crates/corelink-pat/src/argon.rs:133 | `if let Some(params) = phc.params.iter().next()` binds then `let _ = params` discards; dead iterate-and-discard block.
LA3 | CONFIRMED | crates/corelink-pat/src/mint.rs:223 | `let _ = PatHash::from_phc_string(String::new()); // type assertion` — result discarded, dead statement present.
LA4 | CONFIRMED | crates/corelink-pat/src/scopes.rs:1 | Doc says "13 canonical … 51 reserved"; constants define twelve bits (0..=11), so 12 used, 52 reserved.
LA9 | CONFIRMED | worker/src/index.ts:1699 | Worker base64urlDecode re-pads and atobs (accepts non-canonical trailing bits); Rust URL_SAFE_NO_PAD.decode rejects them.
LC3 | CONFIRMED | crates/corelink-reapi/src/handler/bytestream.rs:221 | `read_blob` fetches full body first; `slice_for_offset_limit(body, …)` slices the already-buffered blob afterwards.

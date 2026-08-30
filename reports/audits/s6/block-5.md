---
**Output:**

MI2 | CONFIRMED | worker/src/index.ts:1327 | Computes SHA-256 of raw token bytes before HMAC/D1. | WHY: NONE
MI4 | CONFIRMED | crates/corelink-hash/src/digest.rs:67 | `hex::encode` allocates a `String` for `to_hex()`. | WHY: NONE
MI5 | REFUTED   | -                          | `parse_digest` does not re-split input.              | WHY: NONE
MI7 | CONFIRMED | (multiple files)           | Multiple crates use `reqwest::blocking` inline.     | WHY: NONE

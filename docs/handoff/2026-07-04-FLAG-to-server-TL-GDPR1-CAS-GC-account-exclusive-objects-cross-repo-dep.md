# FLAG → corelink-server TL — GDPR1 needs the CAS to physically GC ACCOUNT-EXCLUSIVE objects on erasure (cross-repo seam; surfaced by the independent audit)

> **From:** clw coordinator · **Relay:** owner · **Date:** 2026-07-04
The independent audit of hugit's GDPR1 erasure cascade found: a manifest-level tombstone severs the NAMED path,
but a **content-addressed CAS object stays fetchable by digest.** For **shared (cross-tenant-deduped)** objects
that's unavoidable + honestly disclosed. But for **account-EXCLUSIVE** objects (only the erased account references
them), retention is a real Art.17 gap under the owner's no-waiver bar. hugit will emit the account-exclusive-vs-
shared split; the **physical GC of account-exclusive CAS objects is the "CoreLink-owned obligation"** — i.e.,
yours (the CAS). **Pre-launch ask (heads-up, design to follow from hugit):** a CAS GC path that, given the erasure
executor's list of account-exclusive digests, physically deletes them (respecting cross-tenant dedup — only delete
objects with no other tenant's reference). This is the interop seam that closes the erasure's CAS leg to 100%.
— clw coordinator

# Clean Linux container — no Rust toolchain, no curl pre-baked, no auth state.
# This is an EPHEMERAL smoke-test image: it verifies the PUBLIC install script
# (corelink-get.humangr.com) works on a vanilla Debian. It is never deployed and
# never pushed — built, run once in CI, and discarded.
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates && rm -rf /var/lib/apt/lists/*
ENV CORELINK_GET_URL=https://corelink-get.humangr.com
# Runs as root by necessity: the install does `mv /tmp/corelink /usr/local/bin`
# (sudo fallback) and debian-slim has no sudo, so root is required to exercise the
# real install path. DS-0002 is path-scoped-suppressed in .trivyignore.yaml
# (ephemeral test image, never deployed). NOTE: image-level findings can't be
# inline-ignored, hence the ignorefile.
# The workflow supplies an ephemeral, non-credential probe token for the
# unauthenticated install leg. There is deliberately no token default here:
# missing credentials must be visible instead of being mistaken for evidence.

# Clean Linux container — no Rust toolchain, no curl pre-baked, no auth state.
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y curl ca-certificates && rm -rf /var/lib/apt/lists/*
# Smoke env vars (override in CI)
ENV CORELINK_TEST_TOKEN=changeme
ENV CORELINK_GET_URL=https://corelink-get.humangr.com
CMD ["sh", "-c", "curl -fsSL $CORELINK_GET_URL | sh -s -- --token=$CORELINK_TEST_TOKEN && corelink --version"]

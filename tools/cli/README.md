# corelink CLI

The `corelink` binary wraps the CoreLink content-addressable cache (CAS) and
action cache (AC) HTTP API at `https://corelink-api.humangr.com`.

## Install

```bash
cargo install --path tools/cli
```

Or build from source:

```bash
cargo build -p corelink-cli --release
cp target/release/corelink /usr/local/bin/
```

## Quick start

### 1. Login

```bash
corelink login --token=corelink_pat_<id>.<secret>.<hmac>
# Saved to ~/.corelink/config.toml (0600 perms)
# tenant_id cached automatically via GET /v1/users/me
```

### 2. Who am I?

```bash
corelink whoami
# tenant_id:    ten_01JXXXXXXXXXXXXXXXXXX
# token_prefix: corelink_pat_ABCDEF***
# route_kind:   pat
```

### 3. Upload a blob

```bash
corelink put myfile.tar.gz
# Uploaded 4096 bytes from 'myfile.tar.gz' → sha256: <64-hex-chars>
```

Large files are streamed in 256 KiB chunks; SHA-256 is computed on the fly
without loading the entire file into memory.

### 4. Download a blob

```bash
corelink get <sha256> -o output.tar.gz
# Downloaded 4096 bytes to 'output.tar.gz' (verify: ok)

# or pipe to stdout
corelink get <sha256> | tar xz
```

Client-side SHA-256 verification is always on (CTRL-CAS-002).

### 5. Action cache

```bash
# Store a build result
corelink ac put sha256:abcdef... result.json

# Retrieve a build result
corelink ac get sha256:abcdef... -o result.json
```

### 6. Config

```bash
corelink config list           # show all config (PAT redacted)
corelink config get defaults.tenant_id
corelink config set defaults.endpoint https://corelink-api.humangr.com
```

## Auth

PAT is read from (in priority order):
1. `CORELINK_PAT` environment variable
2. `~/.corelink/config.toml` → `[auth].pat`

The `--token` flag on `login` is the only way to write a PAT; passing `--pat`
directly on other commands is rejected (CTRL-CRED-001).

## Config file

```toml
[auth]
pat = "corelink_pat_..."   # written by `login`

[defaults]
tenant_id = "ten_..."      # cached by `login` / `whoami`
endpoint = "https://corelink-api.humangr.com"

[telemetry]
enabled = false            # opt-in, default off (GDPR Art. 25)
```

Permissions are enforced to `0600` on Unix. A world-readable config file
is rejected at load time.

## API endpoints

| Command      | Method | Path                              |
|--------------|--------|-----------------------------------|
| `whoami`     | GET    | `/v1/users/me`                    |
| `put <file>` | PUT    | `/v1/cas/<tenant>/<sha256>`       |
| `get <hash>` | GET    | `/v1/cas/<tenant>/<sha256>`       |
| `ac put`     | PUT    | `/v1/ac/<tenant>/<digest>`        |
| `ac get`     | GET    | `/v1/ac/<tenant>/<digest>`        |

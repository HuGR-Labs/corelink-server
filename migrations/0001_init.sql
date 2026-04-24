-- CoreLink initial schema
-- Aplicar em Neon Postgres via: sqlx migrate run
--
-- Convenções:
--   - digest armazenado como BYTEA (binário, não hex) pra economizar espaço
--   - tenant_id sempre via FK, composite PK pra lookups rápidos
--   - last_accessed pra políticas de eviction/TTL no futuro

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- -------------------------------------------------------------------
-- Tenants (provisionados via Clerk webhook)
-- -------------------------------------------------------------------
CREATE TABLE tenants (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
  clerk_org_id TEXT UNIQUE NOT NULL,
  stripe_customer_id TEXT,
  plan TEXT NOT NULL DEFAULT 'free',          -- free | solo | team | business
  storage_limit_bytes BIGINT NOT NULL DEFAULT 10737418240,  -- 10GB default (free)
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX tenants_stripe_customer_idx ON tenants(stripe_customer_id)
  WHERE stripe_customer_id IS NOT NULL;

-- -------------------------------------------------------------------
-- Action Cache (AC) — mapeia action digest → result digest
-- REAPI: ActionCache service
-- -------------------------------------------------------------------
CREATE TABLE action_cache (
  tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  action_digest BYTEA NOT NULL,
  digest_fn SMALLINT NOT NULL DEFAULT 1,     -- 1=BLAKE3, 2=SHA256
  result_proto BYTEA NOT NULL,                -- ActionResult protobuf serialized
  size_bytes BIGINT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  last_accessed TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (tenant_id, action_digest, digest_fn)
);

CREATE INDEX action_cache_last_accessed_idx ON action_cache (tenant_id, last_accessed);

-- -------------------------------------------------------------------
-- CAS index — lookup rápido pra FindMissingBlobs
-- (dados binários mesmo vivem em R2)
-- -------------------------------------------------------------------
CREATE TABLE cas_blobs (
  tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  digest BYTEA NOT NULL,
  digest_fn SMALLINT NOT NULL DEFAULT 1,      -- 1=BLAKE3, 2=SHA256
  size_bytes BIGINT NOT NULL,
  -- Se o blob é um manifest Merkle (decomposto), aponta pra raiz
  is_manifest BOOLEAN NOT NULL DEFAULT FALSE,
  chunk_count INT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  last_accessed TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (tenant_id, digest, digest_fn)
);

CREATE INDEX cas_blobs_last_accessed_idx ON cas_blobs (tenant_id, last_accessed);
CREATE INDEX cas_blobs_size_idx ON cas_blobs (tenant_id, size_bytes DESC);

-- -------------------------------------------------------------------
-- Manifest chunks — árvore Merkle de blobs decompostos
-- Quando um blob grande é quebrado em chunks de 2MiB, cada chunk
-- tem seu próprio digest e fica aqui o mapeamento.
-- -------------------------------------------------------------------
CREATE TABLE manifest_chunks (
  tenant_id UUID NOT NULL,
  parent_digest BYTEA NOT NULL,               -- digest do manifest
  chunk_index INT NOT NULL,                   -- ordem
  chunk_digest BYTEA NOT NULL,                -- digest do chunk individual
  chunk_size BIGINT NOT NULL,
  PRIMARY KEY (tenant_id, parent_digest, chunk_index),
  FOREIGN KEY (tenant_id, parent_digest, 1)
    REFERENCES cas_blobs(tenant_id, digest, digest_fn) ON DELETE CASCADE
);

CREATE INDEX manifest_chunks_chunk_idx ON manifest_chunks (tenant_id, chunk_digest);

-- -------------------------------------------------------------------
-- Usage events — fonte da verdade pro billing via Stripe Meter
-- -------------------------------------------------------------------
CREATE TABLE usage_events (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
  tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  event_type TEXT NOT NULL,                   -- 'bytes_uploaded' | 'bytes_downloaded' | 'cache_hit' | 'cache_miss'
  value BIGINT NOT NULL,                      -- bytes ou count
  timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  reported_to_stripe BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE INDEX usage_events_unreported_idx ON usage_events (tenant_id, timestamp)
  WHERE NOT reported_to_stripe;

CREATE INDEX usage_events_tenant_time_idx ON usage_events (tenant_id, timestamp DESC);

-- -------------------------------------------------------------------
-- API tokens (além do Clerk JWT — pra CLI / CI / agent-use)
-- -------------------------------------------------------------------
CREATE TABLE api_tokens (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
  tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  token_hash BYTEA NOT NULL UNIQUE,           -- SHA256(token), nunca armazena plaintext
  name TEXT NOT NULL,
  scopes TEXT[] NOT NULL DEFAULT '{}',        -- ['read', 'write']
  last_used_at TIMESTAMPTZ,
  expires_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  revoked_at TIMESTAMPTZ
);

CREATE INDEX api_tokens_tenant_idx ON api_tokens (tenant_id) WHERE revoked_at IS NULL;

-- -------------------------------------------------------------------
-- Trigger pra manter updated_at do tenants
-- -------------------------------------------------------------------
CREATE OR REPLACE FUNCTION set_updated_at()
RETURNS TRIGGER AS $$
BEGIN
  NEW.updated_at = NOW();
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tenants_set_updated_at
  BEFORE UPDATE ON tenants
  FOR EACH ROW
  EXECUTE FUNCTION set_updated_at();

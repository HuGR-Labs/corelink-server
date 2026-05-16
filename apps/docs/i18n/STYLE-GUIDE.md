# CoreLink translator style guide

> Audience: external native-speaker translators (pt-BR, es-419) engaged for the
> CoreLink docs i18n delivery (R-4 / H-16). Read this **before** opening any
> `.xlf` file in the bundle.
>
> Source of truth: en-US (`apps/docs/docs/`). When in doubt, defer to the
> English original — file a question instead of guessing.

---

## 1. Voice and tone

CoreLink docs use the **Stripe Docs voice**: friendly-professional, direct,
second-person. We explain technical infrastructure to developers and SREs who
are smart but busy. Optimise for **clarity** over flourish.

- Prefer short sentences. Cut adverbs.
- Address the reader directly ("you", "vous", "tu" — see locale rules below).
- Avoid marketing superlatives ("best-in-class", "revolutionary"). State facts.
- Code is in English. Comments around code can be translated.

## 2. Locale-specific rules

### pt-BR — Brazilian Portuguese

- **Form of address: "você"** (informal singular). Never "vossa senhoria",
  never "vós", never "tu" (regionalism — too informal/regional).
- Avoid hyper-formal Lusophone constructions ("a vossa instalação"). Use
  "sua instalação".
- Numeric format: `1.234,56` (period-thousands, comma-decimal).
- Currency: USD remains USD (do not convert to BRL).
- Quotation marks: prefer ASCII `"..."` to match source.
- Use Brazilian orthography (post-2009 reform): "ideia" not "idéia",
  "frequente" not "freqüente".

### es-419 — Latin American Spanish (neutral)

- **Form of address: "tú"** (informal singular). Never "vos" (Río de la Plata
  regionalism), never "usted" unless context demands formality.
- Avoid Castilian-specific vocabulary: prefer "computadora" over "ordenador",
  "celular" over "móvil", "video" over "vídeo".
- Avoid Castilian verb forms: never "vosotros" — use "ustedes".
- Numeric format: `1,234.56` is acceptable; `1.234,56` also acceptable. Be
  consistent within a page.
- Currency: USD remains USD.

## 3. Do-not-translate list

The following terms are **product names, technical identifiers, or proper
nouns**. Keep them verbatim in every translation:

### Product and brand

- CoreLink
- HuGR, HumanGR, humangr-labs
- Forge (when used as the customer-zero product name)
- Stripe, Cloudflare, GitHub, Linear, ProZ.com
- Smartling, Transifex, Smartcat, memoQ, Trados

### Technical identifiers

- PAT (Personal Access Token)
- BYOK (Bring Your Own Key)
- CAS (Content Addressable Storage)
- AC, Action Cache
- REAPI, Remote Execution API
- gRPC, HTTP/2, HTTP/3, QUIC
- BLAKE3, SHA-256, BLAKE2b, Ed25519
- D1 (Cloudflare D1)
- KV, R2, Durable Objects, DO
- Workers, Pages, Wrangler
- OAuth, OAuth 2.0, OIDC, SAML, JWT
- LGPD, GDPR, SOC 2, ISO 27001, PCI DSS
- DSR (Data Subject Request)
- DPA (Data Processing Agreement)
- TLS, mTLS, ACME, ALPN
- TPM, HSM, KMS, AES-GCM
- DLP, IAM, RBAC, ABAC
- DAU, MAU, MAA, MRR, ARR, NRR
- Bazel, Buck2, Pants, soong, ninja
- WASM, WASI

### File extensions, protocols, code tokens

- `.mdx`, `.md`, `.toml`, `.yaml`, `.proto`
- `grpc://`, `https://`, `tcp://`
- HTTP status codes (200, 401, 403, 503, etc.)
- Anything inside backticks (`code`) — never translate

## 4. Markdown / MDX rules

| Element | Rule |
|---|---|
| Fenced code blocks (```` ``` ````) | **Never translate.** Marked `translate="no"` in XLIFF. |
| Inline `code` (single backticks) | **Never translate.** Even within prose. |
| URLs and link targets `[text](url)` | Translate **text** only; never the URL. |
| Markdown headings (`#`, `##`, ...) | Translate text; preserve heading level. |
| Frontmatter (`--- ... ---` at file top) | **Never translate.** Marked `translate="no"`. |
| MDX components (`<Tabs>`, `<Admonition>`, `<TabItem>`) | Keep tag verbatim; translate text content inside. |
| MDX props (`<TabItem value="x" label="Y">`) | Translate `label` only; never `value`. |
| Variable placeholders `{{name}}`, `${VAR}`, `$VAR` | **Never translate.** Keep verbatim. |
| Admonition titles (`:::note`, `:::tip Note`) | Translate the title text after the keyword; keep keyword. |
| HTML entities (`&amp;`, `&lt;`) | Preserve as-is. |
| Comments (`<!-- ... -->`) | **Never translate.** Especially `<!-- i18n:TODO -->` markers. |

## 5. Specific phrasing conventions

| English | pt-BR | es-419 |
|---|---|---|
| "cache hit" | "cache hit" (term of art; keep English) | "cache hit" (idem) |
| "cache miss" | "cache miss" | "cache miss" |
| "build" (noun) | "build" (term of art) | "build" |
| "build" (verb) | "compilar" / "fazer build" | "compilar" / "hacer build" |
| "remote cache" | "cache remoto" | "caché remoto" |
| "ingest" | "ingestão" | "ingestión" |
| "rate limit" | "limite de taxa" | "límite de tasa" |
| "tenant" | "tenant" (term of art) | "tenant" |
| "workspace" | "workspace" | "workspace" |
| "deploy" | "deploy" / "implantar" | "deploy" / "desplegar" |
| "rollback" | "rollback" / "reverter" | "rollback" / "revertir" |
| "self-hosted" | "auto-hospedado" | "auto-hospedado" |
| "managed service" | "serviço gerenciado" | "servicio gestionado" |
| "dashboard" | "dashboard" / "painel" | "panel" / "dashboard" |
| "endpoint" | "endpoint" | "endpoint" |
| "payload" | "payload" | "payload" |
| "token" | "token" | "token" |
| "throughput" | "throughput" / "vazão" | "throughput" / "rendimiento" |
| "egress" | "egress" / "saída" | "egress" / "salida" |

When uncertain between Anglicism and translated term, **prefer the term used
in practice by the local developer community** (read Brazilian/Spanish tech
blogs, not academic linguistic guides).

## 6. Numbers, units, and dates

- Date format in prose: **ISO 8601** when precise (`2026-05-14`). When
  human-friendly is needed, use locale convention (pt-BR: `14 de maio de
  2026`; es-419: `14 de mayo de 2026`).
- Time: 24-hour for technical contexts, 12-hour acceptable in prose.
- Units: keep SI (ms, MB, GB). Do not convert.
- Percentages: use `%` symbol with no space (`99,9%` pt-BR; `99.9%` es-419).

## 7. Banned constructions

| Banned | Why | Use instead |
|---|---|---|
| "favor" (as in "favor traduzir") | Brazilian formal-bureaucratic | "por favor" or direct imperative |
| "no obstante" | Castilian formal | "sin embargo" |
| "vossa senhoria" | archaic Portuguese | "você" |
| "vosotros" | Castilian-only verb form | "ustedes" |
| machine-translation telltales: "Estoy ejecutando" for "running", literal "estado de la técnica" | poor MT signal | natural locale phrasing |
| Untranslated EN word when a clear equivalent exists | brand inconsistency | translated form |
| Translated technical brand name (e.g. "Trabajadores" for "Workers") | breaks search/docs | keep English |

## 8. Quality gate

After submission, our `translation-quality-check.ts` will scan for:

- Empty `<target>` segments (unfinished work).
- Banned terms from §3 appearing in `<target>` (e.g. translated "PAT" or
  "BLAKE3").
- Banned constructions from §7.
- Broken MDX structure (mismatched `<Tabs>` / `</Tabs>`, etc.).
- Changed code blocks vs source.
- Broken inline link targets.

Failures will be flagged in `apps/docs/dist/translation-quality-report.md`
and returned to the translator for revision under the contract SLA.

## 9. Contact

- Engineering point of contact: docs@humangr.com (internal)
- Style questions during translation: file as a comment in the XLIFF segment
  using `<note category="translator-question">`.

---

**Last revised:** 2026-05-14. Style-guide owner: HuGR docs/i18n team.

# B-314 / #1681 — rascunho de decisão Legal/DPO sobre Sigstore

**Estado:** `pending` — rascunho não assinado, sem aprovação jurídica e sem efeito
sobre o aviso publicado. **Owner:** owner, com coordenação Legal/DPO. **Data de
captura:** 2026-09-22. **Issue:** #1681.

Este documento prepara a decisão exigida pelo pacote B-314. Não seleciona um
resultado, não é assinatura, não autoriza publicação e não notifica clientes. A
issue permanece aberta até que Legal Counsel e DPO escolham e assinem uma
opção com a evidência indicada abaixo.

## Campos de decisão

```yaml
schema_version: 1
finding: B-314
issue: 1681
state: pending
owner: owner
captured_at: 2026-09-22
legal_reviewer: null                 # nome + data somente após revisão
dpo_reviewer: null                  # nome + data somente após revisão
selected_outcome: null               # remove_sigstore_row | retain_and_document_transfer
notice_version: null                 # versão efetivamente aprovada
transfer_basis: null                 # obrigatório se retain_and_document_transfer
recipient_scope: null                # obrigatório se retain_and_document_transfer
data_category_scope: null            # obrigatório se retain_and_document_transfer
approved_notice_wording: null        # texto aprovado, ou referência versionada
published_diff: null                 # referência ao diff de quatro localidades
signed_artifact_sha256_or_reference: null
effective_timestamp: null
```

Os dois valores permitidos para `selected_outcome` são:
`remove_sigstore_row` e `retain_and_document_transfer`. Até o preenchimento dos
campos de revisão e do artefato assinado, o estado deve continuar `pending`.

## Fatos verificados e limites

- As quatro tabelas de transferência têm uma única linha combinada
  `PagerDuty / GitHub / Sigstore`, com `US`, `DPF + SCC + sub-processor-specific
  posture` e `Operational metadata; no end-user PII`: os quatro arquivos de
  locale estão listados em `docs/handoff/2026-09-06-b314-gdpr-sigstore-transfer.json`
  e são protegidos por `scripts/verify_b314_gdpr_sigstore.py`. O frontmatter da
  cópia em inglês continua `draft: true`; este documento não trata esses
  arquivos como uma publicação aprovada.
- O Trust Center, o gerador, o registro de vendors e o disclosure contratual
  agora usam a formulação factual comum: os caminhos release-SLSA, CAS e TSA
  enviam somente metadados de artefato/assinatura próprios da CoreLink; não há
  caminho de dados de clientes ligado a Sigstore. A seam separada de
  transparency-log não é transporte ativo; qualquer uso futuro com payload
  pseudonimizado exige nova revisão Legal/DPO. A correção também sincroniza a
  data de atualização para 2026-09-22. Isso não escolhe o resultado da tabela
  GDPR e não autoriza publicação adicional.
- O fluxo release-SLSA monta sujeitos com nomes de artefatos e hashes SHA-256,
  mais referência de tag, commit e invocation, e envia a proveniência assinada
  a Fulcio/Rekor (`.github/workflows/release-slsa3.yml:282-375`). O fluxo CAS
  assina SBOM CycloneDX (`.github/workflows/cas_foundation.yml:278-360`).
- O cliente TSA envia o hash SHA-256 do SBOM e um nonce
  (`.github/workflows/sbom.yml:208-251`, `tools/sbom-publish/src/tsa.rs:78-112`).
  Isso descreve os fluxos observados; não prova sozinho a ausência de dados de
  clientes em todo uso futuro ou histórico.
- O crate separado de transparency log constrói uma entrada com digest,
  assinatura destacada e chave pública, e sua documentação permite payload
  pseudonimizado; o transporte HTTPS real continua deferido
  (`crates/corelink-transparency-log/src/entry.rs:7-13,68-90`,
  `crates/corelink-transparency-log/src/lib.rs:40-50`). Portanto, não se deve
  transformar a existência do código em uma conclusão jurídica sobre uma
  transferência.
- O registro de revisão de Sigstore continua `TEMPLATE`, com reviewer, DPA,
  mecanismo SCC/transferência, TIA e resultado em `TBD`
  (`docs/compliance/vendor-reviews/sigstore-review-2026-04.md:1-23`). A matriz
  SCC registra Sigstore como `N/A` / fora do escopo de SCC
  (`specs/_compliance/GDPR-SCC-EXECUTION-2026-05-15.md:179-188`). Esses itens
  são lacunas de evidência, não aprovação nem fundamento para manter a linha.

## Opções para Legal/DPO

| Resultado | Alteração após assinatura | Condições mínimas | Risco se adotado sem completar as condições |
|---|---|---|---|
| `remove_sigstore_row` | Remover somente Sigstore das quatro tabelas, preservando PagerDuty e GitHub; anexar o diff exato por locale. | Confirmar que o aviso não precisa listar o uso de supply-chain; registrar a decisão assinada, versão do aviso e data efetiva. | A remoção pode ocultar um fluxo que Legal/DPO entendam como transferência; não deve ser feita com base apenas na existência do gerador ou workflow. |
| `retain_and_document_transfer` | Dividir a linha combinada em PagerDuty/GitHub e Sigstore; publicar apenas depois de aprovar o texto. | Nomear recipiente/serviço, categorias de dados, base ou mecanismo de transferência, escopo, TIA/DPA aplicável e wording do aviso. | A linha atual atribui a Sigstore o mecanismo e as categorias dos outros serviços; mantê-la sem separação perpetua uma afirmação não demonstrada. |

A escolha não decide B-005, B-112, B-118 ou a manutenção/remoção de
`cosign-sign.yml`; esses itens continuam fora do escopo de #1681.

## Checklist de evidência para assinatura e fechamento

- [ ] Legal Counsel identificado, revisou as quatro localidades e assinou a
      disposição.
- [ ] DPO identificado, revisou as quatro localidades e assinou a disposição.
- [ ] Resultado é exatamente `remove_sigstore_row` ou
      `retain_and_document_transfer`.
- [ ] Para retenção: recipiente/serviço, categorias, mecanismo, TIA/DPA e texto
      do aviso estão preenchidos e versionados.
- [ ] Para remoção: diff exato das quatro localidades preserva PagerDuty e
      GitHub e tem revisão de Legal/DPO.
- [ ] `notice_version`, `published_diff`, `effective_timestamp` e referência ou
      hash do artefato assinado estão preenchidos.
- [ ] Qualquer mudança nas páginas públicas passou pelo PR próprio e pelo gate
      de docs; o Trust Center só é atualizado após a decisão aplicável.
- [ ] O owner executou `python3 -S scripts/verify_b314_gdpr_sigstore.py` no
      estado apropriado, reconciliou `BACKLOG.md` e anexou a evidência à issue.

## Referências

- `docs/handoff/2026-09-06-b314-gdpr-sigstore-transfer.json`
- `scripts/verify_b314_gdpr_sigstore.py`
- `apps/docs/docs/explanation/privacy/gdpr.mdx` e os três locales traduzidos
- `apps/docs/docs/trust/subprocessors.mdx`
- `scripts/gen-public-subprocessors.py`
- `legal/sub-processors.md`
- `docs/compliance/vendor-reviews/sigstore-review-2026-04.md`
- `specs/_compliance/GDPR-SCC-EXECUTION-2026-05-15.md`
- `crates/corelink-transparency-log/src/{entry.rs,lib.rs}`

**Refs #1681.**

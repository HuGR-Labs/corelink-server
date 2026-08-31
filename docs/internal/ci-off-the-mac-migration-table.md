# WP-CI — tirar o CI da máquina do owner: a tabela de decisão

Medido em **2026-08-31** contra `origin/main` @ `af42b328`. Esta página é a
**decisão**; os PRs são execução. Nada foi editado antes dela.

---

## 0. Recontagem (obrigatória — feita, e ela confirma o inventário do contrato)

```
 96 + 16(com comentário) = 112  runs-on: corelink
 76                             runs-on: [self-hosted, mac, corelink-builder]
 18                             runs-on: ubuntu-latest
  3                             runs-on: ubuntu-x64-4core
  1                             runs-on: ${{ schedule && ubuntu-latest || mac }}   (secrets-drift)
```

**55 workflows distintos** carregam um desses: **45 no Mac** (76 jobs) e **10 hosted**
(18 jobs; `secrets-drift` aparece nos dois lados porque alterna por evento).

---

## 1. A frota `corelink` — inventário MEDIDO, não presumido

Fonte: `runner-probe.yml` **run 31454999820, 2026-08-11** (o arquivo do workflow já
não está na `main`; as execuções estão). Controle positivo ao lado: `rustfmt.yml`,
15 execuções, última verde 2026-08-31 — a consulta enxerga.

| | |
|---|---|
| CPU / RAM | **4 vCPU (AMD EPYC) / 12,5 GB** |
| Disco | **18 GB totais, ~15 GB livres no início** (`/dev/vdc`) |
| OS | Ubuntu 24.04.4 LTS x86_64 |
| Rust | `1.91.1` + `1.96.0` (default), ambos **linux-gnu**. `wasm32` só no 1.91.1 |
| **PRESENTE** | `cargo` `rustc` `rustup` `clippy` `rustfmt` `sccache 0.17` `cargo-deny 0.19.8` `cargo-audit 0.22.2` · `node 22.23.2` `npm 10.9.8` `pnpm 10.32.1` `npx` · `python3 3.12.3` · `jq` `git` `curl` `unzip` `tar` `make` `cc 13.3` `ld 2.42` `apt-get` `sudo` · `clw 0.1.4` |
| **AUSENTE** | **`docker`** · **`gh`** · **`go`** · `java` · `pip3` · `wget` · **nenhum toolchain `nightly`** · **nenhum `llvm-tools`** · `cargo-fuzz` / `cargo-mutants` não instalados |

Duas correções ao que está escrito no repo hoje:

- `docs/internal/ci-runner-fabric-box.md` §3/§5 diz que **node/npm/pnpm/python3 estão
  ausentes**. **Isso é falso desde a rolagem de imagem de 2026-08-03.** A sonda acima é
  a autoridade.
- `api-reference-sync.yml:64` afirma «`gh` 2.97.0 baked at increment 3 (#453)».
  **A sonda diz `gh ABSENT`.** Puxei o log da única execução `push` desse workflow
  (32784843655): ela terminou com `changed=false` e **pulou o passo que chama `gh`**.
  Ou seja: a afirmação nunca foi exercitada — é um verde que não prova nada. **Trato
  `gh` como AUSENTE até que uma execução o exercite.** É o eixo do Grupo C abaixo.

Controles positivos para as ações de setup (a frota **tem** tool-cache gravável):
`terraform-lint.yml` (`hashicorp/setup-terraform`) verde 2026-08-11; `docs-ci.yml`
(`actions/setup-node` + `pnpm`) verde 2026-08-31.

---

## 2. CI-Q4 — o que 76 jobs a mais fazem com a frota. Diga antes de mandar.

**Fila.** A frota carrega hoje ~112 jobs. A migração completa a leva para **~205**. A
frota é **efêmera**: uma box por job, destruída ao fim. O modo de falha conhecido não é
lentidão — é **spawn recusado/perdido deixa o job `queued` para sempre**, e
`timeout-minutes` **não** limita tempo em fila (só começa a contar quando o job está
`running`). Isso não gera falso-verde (o `pre-merge-gate-check.sh` pontua pendente como
⛔), mas **dobrar a demanda dobra a exposição a esse travamento**. Mitigação: mover em
ondas, uma por PR, e observar a fila entre elas.

**Disco — e aqui está o risco real.** 18 GB por box. Medido na sonda: **um único crate**
(`corelink-hash`) leva `~/.cargo` de 415 M → **1002 M** e cria `target/` de **996 M** —
**~1 GB por crate**, com o disco em 5,1 G/13 G livres depois. **Uma build de workspace
inteiro nunca foi medida nesta box.** O repo tem ~73 crates. Seis lanes fazem justamente
isso:

`cas_foundation` (`cargo test --workspace` + `cargo doc`) · `coverage`
(`cargo-llvm-cov` instrumenta o workspace inteiro) · `codeql` (`cargo build --workspace`
+ banco CodeQL) · `nightly` (`cargo mutants --workspace`) · `workspace-lint` e
`welcome-first-pr` (`cargo clippy --workspace`).

**Essas seis vão por último, uma de cada vez, cada uma com `df -h` antes e depois, e a
primeira execução É a medição.** Foi exatamente esse perfil que encheu
`/opt/actions-runner/…` hoje. Mandar as seis juntas troca o problema de lugar.

---

## 3. ⚠️ A armadilha que a tabela existe para mostrar

Nove workflows têm o caminho do toolchain **darwin escrito à mão**:

```yaml
run: echo "$HOME/.rustup/toolchains/nightly-x86_64-apple-darwin/bin" >> "$GITHUB_PATH"
```

Trocar só o `runs-on:` **não** quebra esse passo — `echo … >> $GITHUB_PATH` sai 0 com um
diretório inexistente. O job fica **verde** e o `cargo fuzz` seguinte roda com o cargo
errado ou morre com `command not found`. É a diferença entre migrar e *parecer* migrar.

E o conserto **não é trocar `apple-darwin` por `unknown-linux-gnu`**: a box **não tem
toolchain `nightly` nenhum** (sonda: só `1.91.1` e `1.96.0` stable). O conserto correto é
`rustup toolchain install nightly` + `cargo install cargo-fuzz` **por execução**, numa box
efêmera — custo real, não uma linha.

### Refutação da contagem preliminar (~35 / 9 / 1)

Confirmo os **9 com caminho darwin** e o **1 macOS-real**. Refuto o resto: **`gh` está
ausente da box**, e 6 workflows do Mac o invocam em YAML vivo (não em comentário) — eles
não são troca de uma linha. E `perf-nightly`/`perf-regression` não podem mover por
semântica de gate, não por ferramenta. A partição medida é:

| classe | nº |
|---|---:|
| A — troca de uma linha, nada mais | **27** |
| B — + caminho do toolchain darwin/nightly | **7** |
| C — + `gh` ausente da box | **6** |
| Z — **não migra** (macOS real ou semântica do gate) | **5** |
| | **45** |

---

## 4. A TABELA — 45 do Mac

Legenda de «conserto além do runs-on»: **—** = nada; **TC** = caminho de toolchain
darwin/nightly; **GH** = precisa de `gh`; **SEM** = mudança de semântica do gate.

### Grupo A — `runs-on:` e mais nada (27 workflows, 40 jobs) → PR **G-A**

| workflow | jobs | atual | alvo | conserto | motivo |
|---|---:|---|---|---|---|
| `action-sha-audit.yml` | 1 | mac | `corelink` | — | python3 puro |
| `actionlint.yml` | 1 | mac | `corelink` | — | baixa actionlint+shellcheck do release pinado; já auto-detecta o SO |
| `api-deprecation-check.yml` | 1 | mac | `corelink` | — | python3 puro |
| `canonical-consistency.yml` | 1 | mac | `corelink` | — | python3 puro |
| `changelog-validate.yml` | 1 | mac | `corelink` | — | python3 + bash |
| `corelink-reapi.yml` | 1 | mac | `corelink` | — | cargo por crate; os outros 2 jobs já estão na frota |
| `dashboard_validation.yml` | 1 | mac | `corelink` | — | python3 puro |
| `dco-check.yml` | 1 | mac | `corelink` | — | git + bash |
| `docs-reality.yml` | 1 | mac | `corelink` | — | python3 puro |
| `e2e-admin-ui-render.yml` | 1 | mac | `corelink` | — | node/pnpm/playwright — todos na imagem |
| `e2e-clerk-signup.yml` | 2 | mac | `corelink` | — | node/pnpm/python3/curl/jq |
| `e2e-stripe-checkout.yml` | 2 | mac | `corelink` | — | node/pnpm/python3/wrangler |
| `i18n-stale.yml` | 1 | mac | `corelink` | — | jq + `actions/github-script` (JS, roda na box) |
| `license-policy.yml` | 1 | mac | `corelink` | — | cargo + `taiki-e/install-action` |
| `neon-shadow-reconcile-daily.yml` | 1 | mac | `corelink` | — | python3 puro |
| `proptest-density-gate.yml` | 1 | mac | `corelink` | — | bash puro |
| `quickstart-validate.yml` | 1 | mac | `corelink` | — | bash puro |
| `region_pinning.yml` | 4 | mac | `corelink` | — | python3 + cargo por crate; 4 jobs irmãos JÁ estão na frota |
| `sbom.yml` | 5 | mac | `corelink` | — | cargo-cyclonedx via `taiki-e/install-action` + python3 |
| `secrets-drift.yml` | 1 | condicional | `corelink` | — | python3; **e mata a expressão condicional inteira** (§5) |
| `shell-pipeline-safety.yml` | 1 | mac | `corelink` | — | python3 puro (a menção a `docker` é **comentário**) |
| `slo-instrumentation.yml` | 1 | mac | `corelink` | — | python3 puro |
| `spec_validation.yml` | 1 | mac | `corelink` | — | python3 puro |
| `stale.yml` | 1 | mac | `corelink` | — | só `actions/stale` (JS) |
| `terraform-drift.yml` | 3 | mac | `corelink` | — | `hashicorp/setup-terraform` — **controle: `terraform-lint` verde na frota** |
| `tla_check.yml` | 1 | mac | `corelink` | — | `actions/setup-java` + tla2tools; tool-cache é gravável (mesmo controle) |
| `workspace-lint.yml` | 1 | mac | `corelink` | — | `cargo clippy --workspace` → **onda de disco, vai por último** (§2) |

### Grupo B — caminho do toolchain darwin/nightly (7 workflows, 16 jobs) → PR **G-B**

| workflow | jobs | atual | alvo | conserto | motivo |
|---|---:|---|---|---|---|
| `corelink-client-verify.yml` | 3 | mac | `corelink` | **TC** ×2 | 4 jobs irmãos já na frota; 2 passos colam o path darwin |
| `corelink-hash.yml` | 3 | mac | `corelink` | **TC** ×2 | idem; `fuzz-smoke` + `fuzz-nightly` |
| `corelink-meta.yml` | 3 | mac | `corelink` | **TC** ×2 | idem |
| `corelink-worker.yml` | 3 | mac | `corelink` | **TC** ×2 | idem |
| `fuzz-nightly.yml` | 1 | mac | `corelink` | **TC** ×1 | `cargo fuzz` — nightly + cargo-fuzz por execução |
| `nightly.yml` | 4 | mac | `corelink` | **TC** ×1 | `cargo mutants --workspace` → **também onda de disco** (§2) |
| `tenant-path.yml` | 3 | mac | `corelink` | **TC** ×2 | 1 job irmão já na frota |

Conserto por ocorrência, idêntico nos 7: substituir o `echo …apple-darwin…` por
`rustup toolchain install nightly --profile minimal` + usar `cargo +nightly`. **Um passo
que falha alto se o nightly não resolver** — nunca um `echo` que sai 0.

### Grupo C — bloqueados por `gh` ausente (6 workflows, 9 jobs) → PR **G-C**

| workflow | jobs | atual | alvo | conserto | motivo |
|---|---:|---|---|---|---|
| `dependabot-auto-merge.yml` | 1 | mac | `corelink` | **GH** | `gh pr merge` |
| `dependabot-policy.yml` | 2 | mac | `corelink` | **GH** | `gh` + cargo-deny |
| `pr-labels.yml` | 2 | mac | `corelink` | **GH** | `gh` + `actions/labeler` |
| `welcome-first-pr.yml` | 1 | mac | `corelink` | **GH** | implementação nativa em `gh` — feita justamente para fugir de uma container action |
| `release-slsa3.yml` | 3 | mac | `corelink` | **GH** | `gh release` + cosign |
| `sign-linux.yml` | 2 | mac | `corelink` | **GH** | `gh` + `gpg`; assina artefato **Linux** — nada aqui exige macOS |

Duas saídas, e a escolha é sua: **(a)** pedir `gh` na imagem da frota (resolve os 6 de
uma vez, e valida a afirmação de `api-reference-sync.yml` que hoje é falsa ou não
exercitada); **(b)** um passo `install gh` pinado por workflow (custo por execução ×6).
**Recomendo (a)** e um item de backlog para (b) como ponte. Este PR **não é escrito até
a escolha** — escrevê-lo antes é adivinhar.

### Grupo Z — NÃO migra (5 workflows, 11 jobs)

| workflow | jobs | atual | alvo | motivo — escrito, não presumido |
|---|---:|---|---|---|
| `notarize-macos.yml` | 2 | mac | **fica no Mac** | `codesign` + `xcrun notarytool` + `stapler` + `security unlock-keychain`. **macOS real.** É o único dos 45 que é. |
| `release-cli.yml` | 2 | mac | **fica no Mac (parcial)** | Uma matriz com linux/windows/darwin no mesmo `runs-on`. As linhas `*-apple-darwin` compilam **nativo em macOS** (o comentário registra um bug de linker iconv em cross). As linhas linux/windows **poderiam** sair — exige partir a matriz em dois jobs. **Escopo próprio, não este WP.** |
| `reproducible-build.yml` | 1 | mac | **fica no Mac (decisão sua)** | `TARGET: x86_64-apple-darwin`. Move-se tecnicamente, mas então prova a reprodutibilidade do binário **Linux**, não do que hoje prova. **SEM** — é uma troca de cobertura, e é chamada de produto. Foi reabilitada em 2026-08-24 com a primeira verde da vida (32726344224); não a mexo sem sua palavra. |
| `perf-regression.yml` | 1 | mac | **fica no Mac** | Baselines Criterion são CPU-bound e foram capturadas neste hardware. Trocar o runner **invalida o gate**. Já documentado em `ci-runner-fabric-box.md` §8.4. **SEM.** |
| `perf-nightly.yml` | 2 | mac | **fica no Mac** | `cargo bench` com a mesma dependência de baseline. **SEM.** |

> Custo dos que ficam: `notarize-macos` (última execução 2026-05-29, falha),
> `release-cli` e `release-slsa3`/`sign-linux` (todas 2026-05-29, falha) são lanes de
> **release**, disparadas por tag — não é carga contínua no Mac. `perf-nightly` +
> `perf-regression` são as únicas que rodam com frequência e ficam. **Depois da migração
> o Mac carrega 2 lanes recorrentes + 4 de release, contra 76 jobs hoje.**

---

## 5. A TABELA — 10 hosted (18 jobs) → PR **G-H**

| workflow | jobs | atual | alvo | conserto | motivo |
|---|---:|---|---|---|---|
| `cas_foundation.yml` | 5 | 1× `ubuntu-x64-4core` + 4× `ubuntu-latest` | `corelink` | — | **O motivo escrito morre na frota:** o comentário diz «4-core: o link do workspace OOMa em **2-core**». A box `corelink` é **4 vCPU / 12,5 GB** — o dobro de núcleos e ~o dobro de RAM do `ubuntu-latest` privado (2 vCPU / 7 GB). A restrição era contra `ubuntu-latest`, **não** contra a frota. Resta o **disco de 18 GB** (§2): vai por último, sozinho, com `df -h`. |
| `coverage.yml` | 1 | `ubuntu-x64-4core` | `corelink` | **llvm-tools** | Mesmo argumento de núcleos. Mas a box **não tem `llvm-tools-preview`** (sonda) → `rustup component add`. E `cargo-llvm-cov` instrumenta o workspace inteiro: **maior risco de disco da lista**. Vai por último. |
| `mutation-nightly.yml` | 1 | `ubuntu-x64-4core` | `corelink` | **GH** | Comentário diz «crates pesados ~4,5 h; 2-core estoura o teto de 6 h». Em 4 vCPU o tempo é comparável, não pior. O job de agregação **já roda em `corelink`**. Bloqueio real é `gh`, não CPU → entra com o Grupo C. |
| `codeql.yml` | 1 | `ubuntu-latest` | `corelink` | **GH** + disco | `cargo build --workspace` + banco CodeQL. Última execução **falhou** (2026-08-30). Disco + `gh`. Onda final. |
| `semgrep.yml` | 1 | `ubuntu-latest` | `corelink` | — | Semgrep via pip/registry; python3 presente, `pip3` **ausente** → usar o binário pinado. Última execução falhou (2026-08-10) — **investigar antes de mover**, não junto. |
| `ffi-matrix-ci.yml` | 5 | `ubuntu-latest` | `corelink` | **`go` ausente** | Matriz Python × Go × JS. `go` não está na box; `actions/setup-go` resolve (tool-cache gravável, controle `terraform-lint`). **Mas esta lane nunca passou** (nota permanente: parked, `workflow_dispatch`-only) — migrar não conserta isso. **Não gasto PR aqui; item de backlog.** |
| `cas-canary.yml` | 1 | `ubuntu-latest` | **fica hosted** | — | O job irmão já está em `corelink`. Este existe **para sair da nossa rede**: `# datacenter IP genuinely outside our provider`. Movê-lo apaga o que o canário mede. 1 job, `schedule`. **Exceção nomeada nº 1.** |
| `smoke-install.yml` | 1 | `ubuntu-latest` | **fica hosted** | — | `installer smoke (real Docker daemon required)`. **`docker` é AUSENTE na frota** (sonda + comentário em `cargo-deny.yml:102`). **Exceção nomeada nº 2.** |
| `cosign-sign.yml` | 2 | `ubuntu-latest` | **veredito, não migração** | — | Usa `docker/build-push-action` → precisa de daemon → a frota não serve. Tem waiver de custo do owner. **E tem ZERO execuções na vida** — confirmado contra controle (`rustfmt` = 15 execuções na mesma consulta). **Waiver protege custo, não ausência: é o B-118.** Ver §6. |
| `secrets-drift.yml` | 1 | condicional | `corelink` | — | Já contado no Grupo A. A expressão `${{ schedule && ubuntu-latest || mac }}` **some inteira**: os dois braços viram `corelink`. |

---

## 6. `cosign-sign.yml` — o veredito de três saídas (CI-Q3)

O waiver no cabeçalho responde «por que hosted». Ele **não** responde «por que nunca
rodou». Medido: 0 execuções, contra um controle que devolve 15 na mesma consulta — o
instrumento enxerga, o workflow é que está morto.

As três saídas, e nenhuma é «deixar como está»:
1. **Está morto e ninguém assina release nenhum** → apagar o arquivo, e o gate de
   assinatura que ele deveria cumprir vira item de backlog aberto.
2. **Deveria rodar e o gatilho está errado** → consertar o gatilho; aí ele passa a
   gastar dinheiro de verdade, e o waiver passa a ser exercido pela primeira vez.
3. **A assinatura mudou de dono** (`release-slsa3.yml` também usa cosign) → é
   duplicata; apagar.

Fora do escopo deste WP decidir qual. **Este WP não toca `cosign-sign.yml`** e abre o
item de backlog. Já existe como B-118 — a contribuição aqui é a medição do zero com
controle ao lado.

---

## 7. O agrupamento em PRs

| PR | grupo | workflows | jobs | causa raiz compartilhada |
|---|---|---:|---:|---|
| **G-A1** | A, sem cargo | 20 | 25 | python3/bash/node/jq — a box tem tudo; troca de uma linha |
| **G-A2** | A, com cargo/setup-action | 7 | 15 | idem + `taiki-e`/`setup-terraform`/`setup-java`; só separado porque o blast radius é outro |
| **G-B** | B | 7 | 16 | o caminho de toolchain darwin colado à mão — **um conserto, sete cópias** |
| **G-C** | C + `mutation-nightly` + `codeql` | 8 | 11 | **`gh` ausente da box.** ⛔ **BLOQUEADO** até a decisão (a) imagem vs (b) install por workflow |
| **G-H** | hosted moveáveis | 2 | 2 | `semgrep`, e o que sobrar de hosted após G-C |
| **G-D** | as 6 de workspace inteiro | 6 | — | **disco de 18 GB não medido.** Uma por PR, `df -h` antes/depois, primeira execução = medição |

**G-A1 é grande (20 arquivos) mas é uma linha por arquivo e um `git diff` de 20 linhas
— reviso isso.** Nenhum grupo precisa ser partido por tamanho. O que precisa de decisão
**sua** antes de existir é o **G-C** (`gh`) e o **G-D** (ordem e ritmo das seis pesadas).

---

## 8. O que este trabalho NÃO decide

- **Não decide se `gh` entra na imagem da frota.** É mudança de infra fora do repo.
- **Não decide o destino de `cosign-sign.yml`** (B-118) nem de `ffi-matrix-ci` (parked).
- **Não decide partir a matriz de `release-cli.yml`** para tirar linux/windows do Mac.
- **Não decide se `reproducible-build` troca de artefato provado** (darwin → linux).
- **Não mede uma build de workspace inteiro na box de 18 GB.** Ninguém mediu; a primeira
  execução de cada uma das seis É a medição, e por isso elas vão separadas e por último.
- **Não afirma que a frota aguenta 205 jobs.** Afirma o que se sabe: o modo de falha é
  spawn recusado → `queued` sem teto, e a exposição a ele dobra.

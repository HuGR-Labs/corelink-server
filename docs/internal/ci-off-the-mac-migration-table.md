# WP-CI — tirar o CI da máquina do owner: a tabela de decisão

Medido em **2026-08-31** contra `origin/main` @ `af42b328`. Esta página é a
**decisão**; os PRs são execução. Nada em `.github/workflows/` foi editado antes dela.

> **Revisão 2 (2026-08-31 ~04:10Z) — eu me corrigi, duas vezes.** A revisão 1 afirmava
> que `gh` e `docker` estavam ausentes da frota. **As duas afirmações estavam erradas, e
> o erro era do instrumento, não do repo:** a sonda que li é anterior aos bakes. Ver §1.1
> e §1.3. Consequências: a partição vai de 27/7/6/5 para **33/7/5**, e duas das três
> «exceções hosted» viram **experimento pendente** em vez de bloqueio.
>
> A revisão 2 também mede a premissa do pacote inteiro, que ninguém tinha medido: **o
> label `corelink` não toca nenhum Mac** (§1.0). Se tocasse — e um documento interno
> afirmava que tocava — este WP não moveria nada.

---

## 0. Recontagem (obrigatória — feita, e confirma o inventário do contrato)

```
 112  runs-on: corelink
  76  runs-on: [self-hosted, mac, corelink-builder]
  18  runs-on: ubuntu-latest
   3  runs-on: ubuntu-x64-4core
   1  runs-on: ${{ schedule && ubuntu-latest || mac }}   (secrets-drift)
```

**55 workflows distintos**: **45 no Mac** (76 jobs) e **10 hosted** (18 jobs;
`secrets-drift` aparece nos dois lados porque alterna por evento).

---

## 1. A frota `corelink` — inventário MEDIDO

Fonte: `runner-probe.yml` **run 31454999820, 2026-08-11T03:18Z** (o arquivo do workflow
já não está na `main`; as execuções estão). Controle positivo: `rustfmt.yml`, 15
execuções, última verde 2026-08-31 — a consulta enxerga.

| | |
|---|---|
| CPU / RAM | **4 vCPU (AMD EPYC) / 12,5 GB** |
| Disco | **18 GB totais, ~15 GB livres no início** (`/dev/vdc`) |
| OS | Ubuntu 24.04.4 LTS x86_64 |
| Rust | `1.91.1` + `1.96.0` (default), ambos **linux-gnu**. `wasm32` só no 1.91.1 |
| PRESENTE | `cargo` `rustc` `rustup` `clippy` `rustfmt` `sccache 0.17` `cargo-deny 0.19.8` `cargo-audit 0.22.2` · `node 22.23.2` `npm 10.9.8` `pnpm 10.32.1` `npx` · `python3 3.12.3` (+ PyYAML, requests, jsonschema) · `jq` `git` `curl` `unzip` `tar` `make` `cc 13.3` `ld 2.42` `apt-get` `sudo` · `clw 0.1.4` · **`gh 2.97.0`** (§1.1) |
| AUSENTE | `go` · `java` · `pip3` · `wget` · **nenhum toolchain `nightly`** · **nenhum `llvm-tools`** · **`cargo-fuzz` não instalado** |
| `docker` | **shim, não daemon** — `docker-shim.sh` → `nerdctl`/containerd/BuildKit, assado em `corelink-runners`#459 (2026-08-13). Ver §1.3 |

### 1.0 O alvo é mesmo fora do Mac? Sim — medido no registro de runners

Antes de qualquer outra coisa, a premissa do pacote inteiro. `gh api
/repos/HuGR-Labs/corelink-server/actions/runners`, 2026-08-31 ~04:05Z:

| | |
|---|---:|
| runners com o label **`corelink`** | **27** |
| …dos quais macOS | **0** |
| Mac builders `corelink-builder-1…5` | 5 — labels `self-hosted, macOS, X64, mac, corelink-builder`, **nenhum com `corelink`** |

**Os dois pools são disjuntos por label: `runs-on: corelink` não pode cair no Mac.**
Isso **refuta** a nota de 2026-08-17 em `ci-runner-fabric-box.md` («o label `corelink`
é um pool MISTO … o scheduler escolhe qualquer um»), que, se fosse verdade, invalidaria
este WP inteiro. Corrigido lá em PR **#1495**.

*Alcance declarado:* é o registro **de repositório**; o de organização devolve 403 para
o token em uso. Os 5 Macs e os 27 da frota aparecem no nível de repo, então o pool contra
o qual este repo agenda é o medido.

**E a contenção, na mesma leitura: os 5 de 5 Macs estavam `online` E `busy`.** É essa a
fila que este pacote drena.

### 1.1 ⚠️ A correção: `gh` ESTÁ na imagem, e eu disse o contrário

A revisão 1 desta página dizia «`gh` ABSENT (sonda) — a afirmação em
`api-reference-sync.yml:64` é falsa». **As duas frases estavam erradas.** O que
aconteceu:

- A sonda que li rodou em **2026-08-11T03:18Z**.
- `gh` foi assado na imagem em **`corelink-runners` PR #453, commit `cc9d06b`,
  2026-08-11T17:53 -03:00 = 20:53Z** — **17 horas DEPOIS da sonda**.

A medição era verdadeira sobre o passado e falsa sobre o presente — exatamente o
mesmo defeito que eu tinha acabado de apontar nos comentários dos jobs
`ubuntu-x64-4core`. **Cometi o defeito que estava relatando.**

**Controle positivo, contemporâneo, num job real:** `bot-pr-has-checks.yml`
(`runs-on: corelink`), **run 33347274511, 2026-08-31T01:20Z, success**. Seu stdout:

```
## bot-authored open PRs in `HuGR-Labs/corelink-server`
No open bot-authored PRs. (5 open PRs scanned.)
```

Essa linha **só pode ser produzida por uma chamada `gh api` bem-sucedida** — o script
levanta `RuntimeError` e emite `::error::could not list open PRs` se o `gh` falhar.
Não é «o job ficou verde»: é o efeito observável de o `gh` ter respondido.

**Consequência:** o Grupo C (6 workflows) **não está bloqueado**. Não é preciso PR de
imagem para `gh`, e a frase em `api-reference-sync.yml:64` está **correta**.

**A lição que fica registrada:** *uma sonda tem data. Antes de tratar uma medição como
verdade presente, compare a data dela com a data da última mudança do que ela mediu.*
Nesta frota a imagem «muda sem aviso» — o próprio `ci-runner-fabric-box.md` §7 diz
isso, e foi assim que a Rust foi de 1.91.1 para 1.96.0 em dois dias.

### 1.3 `docker`: existe um shim, e isso NÃO é licença para mover `smoke-install`

Mesma armadilha de data, segundo caso. A sonda de 2026-08-11T03:18Z diz `docker
ABSENT`; um **drop-in** `docker` foi assado em `corelink-runners`#459 (`b6d75be`,
**2026-08-13**), dois dias depois. É `docker-shim.sh` fazendo `exec nerdctl` sobre
containerd + BuildKit — **não** é um daemon Docker. O comentário em
`cargo-deny.yml:102` («the `corelink` box image ships no docker at all») está **stale**
pela mesma razão.

Prova contemporânea de que build de imagem funciona na frota:
`container-build-push-prod.yml` (`runs-on: corelink`) verde em **2026-08-31T00:39** —
**mas ele chama `buildctl` DIRETO** (`--frontend dockerfile.v0`), não passa pelo shim.

**O que NÃO está estabelecido, e eu não vou fingir que está:** se
`docker/build-push-action` (que quer um builder `buildx`) e o `smoke-install`
(«real Docker daemon required») funcionam **através** do shim. Ninguém rodou.
Portanto:

- `smoke-install` e `cosign-sign` **saem de «fica hosted porque não há docker»** e
  entram em **«experimento pendente»**. Ver §5.
- **Um experimento, não um PR de migração**: dispare cada um uma vez em
  `runs-on: corelink` e leia o resultado. Barato, e decide os dois.

### 1.2 O que a imagem realmente NÃO tem

Isto **não** mudou e continua sendo bloqueio real:

| falta | quem precisa | custo medido de assar |
|---|---|---|
| toolchain **`nightly`** | os 7 do Grupo B (`cargo fuzz` exige nightly) | `rustc` 80 MiB + `cargo` 10 MiB + `rust-std` 30 MiB **= 120 MiB xz** (canal `nightly-2026-08-31`) |
| **`llvm-tools`** (no 1.91.1) | `coverage.yml` (`cargo-llvm-cov`) | **35,7 MiB xz** (`llvm-tools-1.91.1-x86_64-unknown-linux-gnu.tar.xz`, `content-length: 37402920`) |
| **`cargo-fuzz`** | os 7 do Grupo B | **879 KiB** — binário musl pré-compilado, `cargo-fuzz 0.13.2`; **não precisa compilar** |

Total do acréscimo baixado: **~156 MiB xz** para os três. Isso é o número que a decisão
pede, e é um argumento a favor de assar: hoje esses 7 workflows pagariam
`rustup toolchain install nightly` + `cargo install cargo-fuzz` (**compilação**, minutos)
**por execução, em box efêmera**. Assar troca um custo recorrente por ~156 MiB de imagem,
uma vez.

> **O que este número NÃO é:** tamanho **instalado** em disco na box. Não medi — não
> tenho box Linux para medir e não vou instalar nightly no Mac do owner a 95% de disco.
> A build da imagem no `corelink-runners` mede isso de graça: o PR de lá inclui um
> `du -sh` da camada. **A primeira build É a medição do tamanho instalado.**

Controles positivos para ações de setup (a frota tem tool-cache gravável):
`terraform-lint.yml` (`hashicorp/setup-terraform`) verde 2026-08-11; `docs-ci.yml`
(`actions/setup-node` + `pnpm`) verde 2026-08-31.

---

## 2. CI-Q4 — o que 76 jobs a mais fazem com a frota

**Fila: ~112 → ~205 jobs.** A frota é efêmera: uma box por job, destruída ao fim. O modo
de falha que importa não é lentidão:

> **Um spawn recusado ou perdido deixa o job `queued` SEM TETO, porque
> `timeout-minutes` só começa a contar quando o job está `running`.** Dobrar a fila de
> ~112 para ~205 dobra a exposição a esse travamento.

Isso não gera falso-verde — `scripts/pre-merge-gate-check.sh` pontua pendente como
⛔ DO NOT MERGE, então uma box perdida **bloqueia** o PR em vez de liberá-lo. Mas é o
argumento que decide o **ritmo**: migrar em ondas, uma por PR, observando a fila entre
elas.

**Disco — o risco real.** 18 GB por box. Medido na sonda: **um único crate**
(`corelink-hash`) leva `~/.cargo` de 415 M → **1002 M** e cria `target/` de **996 M** —
**~1 GB por crate**, disco em 5,1 G usados / 13 G livres depois. **Uma build de workspace
inteiro NUNCA foi medida nesta box.** O repo tem ~73 crates. Seis lanes fazem exatamente
isso:

`cas_foundation` · `coverage` · `codeql` · `nightly` · `workspace-lint` ·
`welcome-first-pr`

**Regra combinada com a guardiã: em série, uma por PR, `df -h` antes e depois, e a
PRIMEIRA execução é EXPERIMENTO, não migração — o número vai para ela antes de a segunda
entrar.** Se a primeira encher o disco, isso não é falha da migração: é **achado sobre a
frota**, vira item no `corelink-runners`, as outras cinco param, e a resposta é disco
maior — não seis lanes brigando por 18 GB.

---

## 3. ⚠️ A armadilha que a tabela existe para mostrar

Nove workflows têm o caminho do toolchain **darwin escrito à mão**:

```yaml
run: echo "$HOME/.rustup/toolchains/nightly-x86_64-apple-darwin/bin" >> "$GITHUB_PATH"
```

Trocar só o `runs-on:` **não quebra esse passo** — `echo … >> $GITHUB_PATH` sai 0 com um
diretório inexistente. O job fica **verde** e o `cargo fuzz` seguinte roda com o cargo
errado ou morre com `command not found`. É a diferença entre migrar e *parecer* migrar.

E o conserto **não é trocar `apple-darwin` por `unknown-linux-gnu`**: a box **não tem
toolchain `nightly` nenhum** (§1.2). O conserto é assar `nightly` + `cargo-fuzz` na
imagem (custo em §1.2) e trocar o passo por um que **falha alto** se o toolchain não
resolver — nunca um `echo` que sai 0.

### Partição medida (revisão 2)

| classe | nº | mudou? |
|---|---:|---|
| A — troca de uma linha, nada mais | **33** | ⬆ +6 (o Grupo C foi absorvido: `gh` existe) |
| B — + `nightly`/`cargo-fuzz` na imagem | **7** | = |
| Z — **não migra** (macOS real ou semântica do gate) | **5** | = |
| | **45** | |

A contagem preliminar do briefing era ~35/9/1. Os **9 com caminho darwin** e o **1
macOS-real** se confirmam. A diferença que sobrevive à correção do §1.1: **2 dos 9 não
migram por outra razão** (`release-cli`, `reproducible-build`), e **`perf-nightly` +
`perf-regression` não migram por semântica de gate, não por ferramenta** — a distinção
mais fácil de errar do lote.

---

## 4. A TABELA — 45 do Mac

### Grupo A — `runs-on:` e mais nada (33 workflows, 49 jobs)

**A1 — python3 / bash / node / jq (20 wf, 25 jobs) → PR `G-A1`**

| workflow | jobs | alvo | motivo |
|---|---:|---|---|
| `action-sha-audit.yml` | 1 | `corelink` | python3 puro |
| `actionlint.yml` | 1 | `corelink` | baixa actionlint+shellcheck do release pinado; já auto-detecta o SO |
| `api-deprecation-check.yml` | 1 | `corelink` | python3 puro |
| `canonical-consistency.yml` | 1 | `corelink` | python3 puro |
| `changelog-validate.yml` | 1 | `corelink` | python3 + bash |
| `dashboard_validation.yml` | 1 | `corelink` | python3 puro |
| `dco-check.yml` | 1 | `corelink` | git + bash |
| `docs-reality.yml` | 1 | `corelink` | python3 puro |
| `e2e-admin-ui-render.yml` | 1 | `corelink` | node/pnpm/playwright — na imagem |
| `e2e-clerk-signup.yml` | 2 | `corelink` | node/pnpm/python3/curl/jq |
| `e2e-stripe-checkout.yml` | 2 | `corelink` | node/pnpm/python3/wrangler |
| `i18n-stale.yml` | 1 | `corelink` | jq + `actions/github-script` (JS, roda na box) |
| `neon-shadow-reconcile-daily.yml` | 1 | `corelink` | python3 puro |
| `proptest-density-gate.yml` | 1 | `corelink` | bash puro |
| `quickstart-validate.yml` | 1 | `corelink` | bash puro |
| `secrets-drift.yml` | 1 | `corelink` | python3; **mata a expressão condicional inteira** — os dois braços viram `corelink` |
| `shell-pipeline-safety.yml` | 1 | `corelink` | python3 puro (a menção a `docker` é **comentário**) |
| `slo-instrumentation.yml` | 1 | `corelink` | python3 puro |
| `spec_validation.yml` | 1 | `corelink` | python3 puro |
| `stale.yml` | 1 | `corelink` | só `actions/stale` (JS) |

**A2 — cargo por crate / setup-actions (7 wf, 15 jobs) → PR `G-A2`**

| workflow | jobs | alvo | motivo |
|---|---:|---|---|
| `corelink-reapi.yml` | 1 | `corelink` | cargo por crate; os outros 2 jobs já estão na frota |
| `license-policy.yml` | 1 | `corelink` | cargo + `taiki-e/install-action` |
| `region_pinning.yml` | 4 | `corelink` | python3 + cargo por crate; 4 jobs irmãos JÁ estão na frota |
| `sbom.yml` | 5 | `corelink` | cargo-cyclonedx via `taiki-e/install-action` + python3 |
| `terraform-drift.yml` | 3 | `corelink` | `hashicorp/setup-terraform` — controle: `terraform-lint` verde na frota |
| `tla_check.yml` | 1 | `corelink` | `actions/setup-java` + tla2tools; tool-cache gravável (mesmo controle) |
| `workspace-lint.yml` | 1 | `corelink` | `cargo clippy --workspace` → **vai no G-D**, onda de disco |

**A3 — os 6 ex-Grupo-C: usam `gh`, que EXISTE (6 wf, 9 jobs) → PR `G-A3`**

| workflow | jobs | alvo | motivo |
|---|---:|---|---|
| `dependabot-auto-merge.yml` | 1 | `corelink` | `gh pr merge` |
| `dependabot-policy.yml` | 2 | `corelink` | `gh` + cargo-deny (ambos na imagem) |
| `pr-labels.yml` | 2 | `corelink` | `gh` + `actions/labeler` |
| `welcome-first-pr.yml` | 1 | `corelink` | implementação nativa em `gh`, feita para fugir de uma container action |
| `release-slsa3.yml` | 3 | `corelink` | `gh release` + cosign |
| `sign-linux.yml` | 2 | `corelink` | `gh` + `gpg`; assina artefato **Linux** — nada aqui exige macOS |

> **Por que A3 é PR separado e não parte do A1**, mesmo sendo a mesma troca de uma linha:
> quatro destes rodam com **token de escrita** — `dependabot-auto-merge` (`pr merge`),
> `pr-labels`, `welcome-first-pr` (`pull_request_target`, comenta em PR de fork) e
> `release-slsa3`/`sign-linux` (assinam release). Mudar **onde** um token de escrita é
> exercido é uma decisão de superfície de ataque, não de conveniência de runner, e
> merece revisão própria. **Este é o único agrupamento aqui feito por risco e não por
> causa técnica** — está declarado de propósito.

### Grupo B — precisa de `nightly` + `cargo-fuzz` na imagem (7 wf, 16 jobs) → PR `G-B`

⛔ **Bloqueado** até o PR de imagem do `corelink-runners` (§1.2) sair.

| workflow | jobs | conserto | motivo |
|---|---:|---|---|
| `corelink-client-verify.yml` | 3 | path darwin ×2 | 4 jobs irmãos já na frota |
| `corelink-hash.yml` | 3 | path darwin ×2 | `fuzz-smoke` + `fuzz-nightly` |
| `corelink-meta.yml` | 3 | path darwin ×2 | idem |
| `corelink-worker.yml` | 3 | path darwin ×2 | idem |
| `fuzz-nightly.yml` | 1 | path darwin ×1 | `cargo fuzz` |
| `nightly.yml` | 4 | path darwin ×1 | `cargo mutants --workspace` → **também G-D** (disco) |
| `tenant-path.yml` | 3 | path darwin ×2 | 1 job irmão já na frota |

### Grupo Z — NÃO migra (5 workflows, 11 jobs)

| workflow | jobs | motivo — escrito, não presumido |
|---|---:|---|
| `notarize-macos.yml` | 2 | `codesign` + `xcrun notarytool` + `stapler` + `security unlock-keychain`. **macOS real.** É o **único** dos 45 que é. |
| `release-cli.yml` | 2 | Matriz com linux/windows/darwin no mesmo `runs-on`. As linhas `*-apple-darwin` compilam **nativo** (comentário registra um bug de linker iconv em cross). As linhas linux/windows poderiam sair — exige partir a matriz. **Escopo próprio.** |
| `reproducible-build.yml` | 1 | `TARGET: x86_64-apple-darwin`. Move-se tecnicamente, mas então prova a reprodutibilidade do binário **Linux** — troca de cobertura, chamada de produto. Reabilitada 2026-08-24 com a primeira verde da vida (run 32726344224). |
| `perf-regression.yml` | 1 | Baselines Criterion são CPU-bound e foram capturadas neste hardware. **Trocar o runner invalida o gate.** Já documentado em `ci-runner-fabric-box.md` §8.4. |
| `perf-nightly.yml` | 2 | `cargo bench`, mesma dependência de baseline. |

> **O que o Mac carrega depois.** `notarize-macos`, `release-cli`, `release-slsa3` e
> `sign-linux` são lanes de **release** disparadas por tag (últimas execuções:
> 2026-05-29) — não é carga contínua. `perf-nightly` + `perf-regression` são as únicas
> recorrentes que ficam. **De 76 jobs para 2 lanes recorrentes + 2 de release.**

---

## 5. A TABELA — 10 hosted (18 jobs)

| workflow | jobs | atual | alvo | motivo |
|---|---:|---|---|---|
| `cas_foundation.yml` | 5 | 1× `4core` + 4× `ubuntu-latest` | `corelink` (**G-D**) | **O motivo escrito morre na frota:** o comentário diz «4-core: o link do workspace OOMa em **2-core**». A box é **4 vCPU / 12,5 GB** — o dobro dos núcleos e ~o dobro da RAM do `ubuntu-latest` privado. A restrição era contra `ubuntu-latest`, **não** contra a frota. Resta o disco. |
| `coverage.yml` | 1 | `ubuntu-x64-4core` | `corelink` (**G-D**) | Mesmo argumento de núcleos. Falta `llvm-tools` na imagem (§1.2). `cargo-llvm-cov` instrumenta o workspace inteiro: **maior risco de disco da lista.** |
| `mutation-nightly.yml` | 1 | `ubuntu-x64-4core` | `corelink` (**G-H**) | «~4,5 h; 2-core estoura o teto de 6 h» — em 4 vCPU o tempo é comparável. O job de agregação **já roda em `corelink`**. `gh` existe. **Desbloqueado.** |
| `codeql.yml` | 1 | `ubuntu-latest` | `corelink` (**G-D**) | `cargo build --workspace` + banco CodeQL. Última execução **falhou** (2026-08-30) — investigar antes, não junto. Disco. |
| `semgrep.yml` | 1 | `ubuntu-latest` | `corelink` (**G-H**) | python3 presente; `pip3` **ausente** → binário pinado. Última execução falhou (2026-08-10) — **investigar antes de mover.** |
| `ffi-matrix-ci.yml` | 5 | `ubuntu-latest` | — | `go` ausente; `actions/setup-go` resolveria. **Mas esta lane nunca passou** (parked, `workflow_dispatch`-only). Migrar não conserta isso. **Item de backlog, não PR.** |
| `cas-canary.yml` | 1 | `ubuntu-latest` | **fica hosted** | O job irmão já está em `corelink`. Este existe **para sair da nossa rede**: `# datacenter IP genuinely outside our provider`. Movê-lo apaga o que o canário mede. **A ÚNICA exceção `ubuntu-*` justificada por desenho** — as outras duas são pendências, não exceções. |
| `smoke-install.yml` | 1 | `ubuntu-latest` | **experimento** | `installer smoke (real Docker daemon required)`. A frota tem um **shim** `docker` (nerdctl) desde 2026-08-13, **não** um daemon — se o instalador sobrevive a isso é **desconhecido**. Uma execução decide. §1.3. |
| `cosign-sign.yml` | 2 | `ubuntu-latest` | **veredito, não migração** | `docker/build-push-action` quer um builder `buildx`; o shim mapeia `buildx build` → `nerdctl build` — **não verificado**. Mas o problema real não é esse: **0 execuções na vida** (§6). Migrar um workflow morto é a ordem errada. |
| `secrets-drift.yml` | 1 | condicional | `corelink` | Contado no A1. |

---

## 6. `cosign-sign.yml` — o veredito de três saídas (CI-Q3)

O waiver responde «por que hosted». **Não** responde «por que nunca rodou». Medido: **0
execuções**, contra um controle que devolve 15 na mesma consulta (`rustfmt`) — o
instrumento enxerga; o workflow é que está morto.

As três saídas, e nenhuma é «deixar como está»:
1. **Morto e ninguém assina release nenhum** → apagar, e o gate de assinatura vira item
   de backlog aberto.
2. **Deveria rodar e o gatilho está errado** → consertar; aí passa a gastar dinheiro de
   verdade e o waiver é exercido pela primeira vez.
3. **A assinatura mudou de dono** (`release-slsa3.yml` também usa cosign) → duplicata,
   apagar.

**Este WP não toca `cosign-sign.yml`.** Já existe como B-118; a contribuição aqui é a
medição do zero com controle ao lado.

---

## 7. O agrupamento em PRs

| PR | wf | jobs | causa raiz | estado |
|---|---:|---:|---|---|
| **G-A1** | 20 | 25 | python3/bash/node/jq — uma linha por arquivo | pronto |
| **G-A2** | 7 | 15 | idem + `taiki-e`/`setup-terraform`/`setup-java` | pronto |
| **G-A3** | 6 | 9 | mesma troca, mas **exercem token de escrita** — agrupado por risco | pronto |
| **G-B** | 7 | 16 | o path darwin colado à mão | ⛔ espera o PR de imagem (`nightly` + `cargo-fuzz`) |
| **G-H** | 2 | 2 | `semgrep` + `mutation-nightly` — cada um com uma falha própria a investigar antes | investigar |
| **G-D** | 6 | — | **disco de 18 GB não medido** — em série, uma por PR, `df -h`, a 1ª é experimento | serial |

Nenhum grupo precisa ser partido por tamanho. G-A1 são 20 arquivos e um diff de 20
linhas.

---

## 8. O que este trabalho NÃO decide

- **Não decide o destino de `cosign-sign.yml`** (B-118) nem de `ffi-matrix-ci` (parked,
  nunca passou).
- **Não decide partir a matriz de `release-cli.yml`** para tirar linux/windows do Mac.
- **Não decide se `reproducible-build` troca o artefato provado** (darwin → linux).
- **Não mede uma build de workspace inteiro na box de 18 GB**, nem o tamanho
  **instalado** do `nightly`. A primeira execução de cada lane pesada, e a primeira build
  da imagem, são as medições.
- **Não afirma que a frota aguenta 205 jobs.** Afirma o modo de falha (spawn recusado →
  `queued` sem teto) e que a exposição dobra.
- **Não decide se `smoke-install` / `cosign-sign` rodam através do shim `docker`.** É um
  experimento de uma execução cada, nomeado em §1.3 e não feito aqui.
- **Não afirma que a imagem de hoje é a de amanhã.** §7 do `ci-runner-fabric-box.md`
  documenta que ela muda sem aviso; §1.1 e §1.3 desta página são a prova de que isso já
  me pegou **duas** vezes no mesmo dia.

---

## 9. PRs abertos por este pacote

| PR | repo | o quê |
|---|---|---|
| **#1488** | `corelink-server` | esta tabela |
| **#1495** | `corelink-server` | corrige `ci-runner-fabric-box.md` — pool misto refutado, §4/§5 desatualizados, sonda inexecutável |
| **#523** | `corelink-runners` | assa `nightly` + `llvm-tools` + `cargo-fuzz` — desbloqueia o Grupo B |

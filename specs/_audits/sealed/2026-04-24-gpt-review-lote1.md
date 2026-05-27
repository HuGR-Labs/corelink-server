# Review Lote 1 — Verification Pass (GPT)

## 1. Veredicto global

Lote 1 melhorou o framework, mas a mensagem do commit vende mais do que o diff entrega. Houve avanço real em lifecycle, refs de glossário, SLSA, `FAILED` no sprint, PRR gate e versionamento. Só que a execução ficou pela metade em pontos estruturais centrais: a separação `doc_status`/`work_status` não foi propagada pelo corpo dos templates, as contagens de seção continuam se contradizendo dentro do próprio framework, a regra de split de WI ficou internamente inconsistente, e o suposto “YAML front matter” não é front matter de verdade nem YAML válido nos 5 templates. Score honesto: **5 resolvidos, 4 parciais, 1 não resolvido, 0 pioraram**. A alegação de “nenhuma regressão” no commit **não se sustenta**.

## 2. Status dos 10 fixes declarados

> Nota metodológica: para fechar a conta dos “10 fixes declarados”, separei o antigo item “1.7 YAML front matter + bloco humano” em **duas verificações independentes**. Isso é necessário porque os dois tiveram resultados diferentes.

### 2.1 Fix 1.1 — Separar `doc_status` x `work_status`

- **Status:** PARCIALMENTE RESOLVIDO
- **Evidência:** `specs/00_framework.md:705-936` reescreve o §7 inteiro, separando explicitamente `doc_status` e `work_status`, com matriz por tipo em `:777-789` e a invariante cruzada em `:827-833`. Os cabeçalhos dos templates também passaram a expor os dois eixos: `specs/_templates/sprint_contract.md:5-42`, `work_item.md:5-31`, `subtask.md:5-31`, `adr.md:5-32`, `production_readiness_review.md:5-45`.
- **Gap remanescente:** o topo fala a linguagem nova, mas o corpo continua falando a velha. Exemplos objetivos: `sprint_contract.md:82` ainda tem `| **Status** | PROPOSED |`; `sprint_contract.md:169-170` ainda exige ADR `ACCEPTED` e predecessor com `Status: SEALED`; `work_item.md:120` ainda usa `Status`, com `ADR-XXXX ... | ACCEPTED |` em `:242` e `:939`; `subtask.md:81` e `:118` repetem `Status`/`ACCEPTED`; `adr.md:43` ainda traz `| **Status** | PROPOSED |`; `production_readiness_review.md:107` ainda usa `| **Status** | NOT_STARTED |`. Em português claro: o sistema agora tem **dois modelos de estado concorrentes dentro dos mesmos arquivos**.
- **O que falta fazer:** reescrever as tabelas de metadata e os checkpoints internos para usar explicitamente `doc_status` e `work_status`, e remover referências operacionais a `Status: ACCEPTED/SEALED` como se ainda fossem o cabeçalho canônico.

### 2.2 Fix 1.2 — Alinhar section counts

- **Status:** PARCIALMENTE RESOLVIDO
- **Evidência:** o framework atualiza os counts canônicos em `specs/00_framework.md:1888-1991`: Sprint `23`, ADR `16`, WI `33`, ST `19`, PRR `23`. Os templates batem com isso no sumário: `sprint_contract.md:49-71` enumera `0–22`; `work_item.md:77-109` enumera `0–32`; `subtask.md:52-70` enumera `0–18`; `production_readiness_review.md:73-95` enumera `0–22`; `adr.md:36-78` + `:82` cobrem `0–15`.
- **Gap remanescente:** o próprio framework continua se desmentindo duas páginas depois. `specs/00_framework.md:1906` ainda diz `Seções principais (32 totais)` para WI, e `:1951` ainda diz `Seções da sub-task (18 totais)`. O count “macro” foi corrigido; o texto explicativo “micro” não.
- **O que falta fazer:** corrigir os rótulos residuais do §34 para `33` e `19`, e fazer uma varredura completa por textos antigos de count em vez de só atualizar os highlights.

### 2.3 Fix 1.3 — Refs de glossário `§15 → §40`

- **Status:** RESOLVIDO
- **Evidência:** `specs/_templates/adr.md:561` agora aponta o glossário master para `00_framework.md §40`; `specs/_templates/sprint_contract.md:368` faz o mesmo. O framework confirma o glossário em `specs/00_framework.md:97-99`.
- **Gap remanescente:** nenhum para este fix específico.

### 2.4 Fix 1.4 — WI split IDs: `REG-WI-SPLIT-001` proíbe `.a/.b`

- **Status:** PARCIALMENTE RESOLVIDO
- **Evidência:** `specs/_templates/work_item.md:1019-1031` abandona explicitamente sufixos `.a`, `.b`, `.1`, `.2` e cria a `REG-WI-SPLIT-001` exigindo IDs sequenciais novos.
- **Gap remanescente:** a regra ficou **internamente contraditória**. Em `work_item.md:1020`, o texto manda marcar o WI original como `doc_status: DEPRECATED` e `supersedes` apontando para os novos WIs. Só que em `:1028-1030` a própria regra já corrige isso e diz que o original deve ser `DEPRECATED` ou `SUPERSEDED`, com `superseded_by` listando os sucessores e os novos WIs usando `supersedes`. Além disso, `:1020` manda “ver `§22 Change Log`”, mas o Change Log real do WI está em `work_item.md:1410` (`## 31. Change Log`). E `PWI` continua aparecendo sem definição em `:1019`.
- **O que falta fazer:** unificar a semântica para `superseded_by` no original e `supersedes` nos novos; corrigir `DEPRECATED` vs `SUPERSEDED` conforme o estado real; arrumar a referência `§22 → §31`; definir `PWI` no glossário ou remover o acrônimo.

### 2.5 Fix 1.5 — Alinhar SLSA

- **Status:** RESOLVIDO
- **Evidência:** o framework continua exigindo `SLSA Level 3 para builds de produção até GA` em `specs/00_framework.md:1632-1634`. O PRR agora alinha isso por faixa de rollout em `specs/_templates/production_readiness_review.md:500-510`: canary até `10%` aceita `≥ 2`; `10–49%` já exige `≥ 3`; `≥ 50%` e `GA` exigem `3`. Os gates concretos aparecem em `:516-520` e `:526-529`.
- **Gap remanescente:** nenhum material. Ficou até mais rígido que o enunciado curto do fix, mas na direção certa.

### 2.6 Fix 1.6 — `FAILED` no sprint header

- **Status:** RESOLVIDO
- **Evidência:** `specs/_templates/sprint_contract.md:10` inclui `FAILED` no `work_status` do YAML; `:37` repete `FAILED` no bloco humano; e `:141-149` mantém `FAILED` na tabela de estados do sprint.
- **Gap remanescente:** nenhum.

### 2.7 Fix 1.7 — YAML front matter nos 5 templates

- **Status:** NÃO RESOLVIDO
- **Evidência:** os 5 templates ganharam **blocos YAML-shaped**, mas não front matter real. Todos começam com título e `Template Version` antes do bloco, por exemplo `sprint_contract.md:1-5`, `work_item.md:1-5`, `subtask.md:1-5`, `adr.md:1-5`, `production_readiness_review.md:1-5`. E o YAML está **fenced** com ```yaml ... ```, então parsers padrão de front matter não vão enxergar isso como metadata de documento. Pior: os templates não parseiam como YAML válido por causa dos placeholders não quoted, por exemplo `owner: {{Sprint Owner}}` em `sprint_contract.md:14`, `owner: {{Nome}}` em `work_item.md:14`, `subtask.md:14`, `adr.md:13`, `production_readiness_review.md:14`. Validação manual com `python3 + yaml.safe_load` falhou nos 5 templates com erro de construção de mapping.
- **Gap remanescente:** este fix, como declarado, **não aconteceu**. O que entrou foi documentação de formato, não front matter parseável.
- **O que falta fazer:** mover o bloco YAML para o topo real do arquivo, fora de code fence, e tornar os placeholders válidos em YAML (`\"{{Nome}}\"`, `\"{{subsistema}}\"`, etc.).

### 2.8 Fix 1.8 — Bloco humano-legível nos 5 templates

- **Status:** PARCIALMENTE RESOLVIDO
- **Evidência:** os 5 templates agora têm bloco humano-legível: `sprint_contract.md:36-42`, `work_item.md:26-31`, `subtask.md:26-31`, `adr.md:23-32`, `production_readiness_review.md:40-45`.
- **Gap remanescente:** o bloco existe, mas na maioria dos templates ele está **incompleto contra a própria regra do framework**. `specs/00_framework.md:898-906` exige `Owner`, `Aprovador Final`, `Revisores`, `Supersedes`, `Superseded By`; `:933-935` adiciona `work_status` e parent para docs de trabalho. Só que `sprint_contract.md:36-42` omite `Revisores`, `Supersedes`, `Superseded By`; `work_item.md:26-31` omite `Aprovador Final`, `Revisores`, `Supersedes`, `Superseded By`; `subtask.md:26-31` omite quase tudo além de `Assignee` e `WI pai`; `production_readiness_review.md:40-45` omite `Owner`, `Aprovador Final`, `Revisores`, `Supersedes`, `Superseded By`.
- **O que falta fazer:** alinhar todos os blocos humanos com o contrato do §7.11 do framework, em vez de publicar uma versão resumida e chamá-la de canônica.

### 2.9 Fix 1.9 — PRR gate desambiguado (`APPROVED` vs `CONDITIONALLY_APPROVED` com `expires_at`)

- **Status:** RESOLVIDO
- **Evidência:** `specs/_templates/production_readiness_review.md:51-59` fixa a regra de rollout por `work_status`; `:679-692` torna caveats de `CONDITIONALLY_APPROVED` dependentes de expiração e re-review; `:718-720` reforça a mesma distinção na decisão final. A ambiguidade do audit v1 sumiu: `CONDITIONALLY_APPROVED` agora vale só para canary `≤ 10%`, e `APPROVED` é o único estado que libera promoção ampla.
- **Gap remanescente:** nenhum no PRR em si. O que sobrou foi drift de apoio em `work_item.md:873-877`, cujo resumo de status do PRR ainda é incompleto e omite `REJECTED`; isso entra como regressão de documentação, não como falha do gate principal.

### 2.10 Fix 1.10 — Version bump `0.2.0 → 0.3.0` com change log

- **Status:** RESOLVIDO
- **Evidência:** `specs/00_framework.md:7-8` e `:20-21` já publicam `version: 0.3.0` / `Versão: 0.3.0`. O change log do framework registra a entrada nova em `specs/00_framework.md:2522-2524`. Os templates também subiram `Template Version` (`sprint_contract.md:3`, `work_item.md:3`, `subtask.md:3`, `adr.md:3`, `production_readiness_review.md:3`).
- **Gap remanescente:** o registro existe e a versão subiu corretamente. O problema é outro: a linha de change log superestima o que o Lote 1 realmente entregou em YAML/front matter.

## 3. Novos problemas / regressões introduzidos

- **Pseudo-front-matter em vez de front matter real.** O patch introduziu um padrão que parece machine-readable, mas não é. Os blocos vêm depois do H1, dentro de code fence, e os 5 templates não parseiam como YAML válido por causa dos placeholders não quoted (`sprint_contract.md:14`, `work_item.md:14`, `subtask.md:14`, `adr.md:13`, `production_readiness_review.md:14`).

- **Split-brain de status dentro dos próprios documentos.** O topo foi migrado para `doc_status`/`work_status`, mas as tabelas internas continuam em `Status`, `ACCEPTED`, `SEALED`. Isso cria duas semânticas concorrentes por arquivo, por exemplo `sprint_contract.md:36-37` vs `:82` e `:169-170`, `work_item.md:26-27` vs `:120`, `:242`, `:939`, `adr.md:23-32` vs `:43`, `:509`, `:575`.

- **Drift novo no §34 do framework.** O framework atualiza os counts “headline” em `00_framework.md:1888-1991`, mas preserva os rótulos antigos `32 totais` e `18 totais` em `:1906` e `:1951`. Resultado: o mesmo arquivo afirma simultaneamente o número novo e o velho.

- **Regra de split de WI contraditória e com referência quebrada nova.** `work_item.md:1020` fala em `supersedes` apontando para sucessores e manda olhar `§22 Change Log`; `:1028-1030` já usa `superseded_by`; e o Change Log real é `## 31` em `:1410`. Isso não existia antes; foi introduzido pelo próprio fix.

- **`AUDIT_PENDING` foi exigido sem schema para armazená-lo.** O framework manda marcar downstream como `AUDIT_PENDING` em `specs/00_framework.md:867`, mas o schema obrigatório proposto em `:879-928` não tem nenhum campo para `audit_status`, `downstream_audit` ou equivalente. O patch criou um requisito novo sem ponto de persistência.

- **O hook de PRR no WI ficou para trás.** `work_item.md:873-877` resume o status do PRR como “Not started / In review / Approved / Conditionally approved” e simplesmente esquece `REJECTED`. O gate principal do PRR ficou bom; o resumo que deveria espelhá-lo não.

## 4. Findings originais não endereçados pelo Lote 1 (com nota do lote planejado)

- **A contradição fundacional do framework continua viva.** `specs/00_framework.md:27` ainda diz que “Nenhum outro documento de spec pode existir sem este estar frozen”, mas o próprio framework segue `doc_status: DRAFT` em `:7` e os templates seguem publicados como canônicos. Isso era finding estrutural do audit v1 e não foi atacado. Pela trilha do commit, isso parece empurrado para o momento de `freeze` real do framework, provavelmente **Lote 6**.

- **A finding de referências quebradas não foi realmente encerrada.** Os refs de glossário foram corrigidos, mas `specs/_templates/work_item.md:1363` ainda manda registrar “status update” em `§19.5`, que continua sendo “Histórico de estimates” em `:991-997`. Isso era higiene de Lote 1, não problema de domínio. **Sem lote declarado; deveria ter sido fechado aqui.**

- **`PWI` continua termo solto, sem definição canônica.** `specs/_templates/work_item.md:1019` usa `Partial Work Item (PWI)` e o termo não aparece formalizado no glossário master. Isso já tinha sido apontado no audit v1 como falha de linguagem ubíqua. Também era higiene estrutural de **Lote 1**, não de domínio.

- **A cobertura real de evidence/role/automation continua desigual fora dos trechos tocados.** O Lote 1 melhorou o PRR §13, mas não resolveu o problema sistêmico que o audit v1 apontou: há várias seções binárias em sprint/WI/PRR fora do padrão estruturado completo. Isso conversa diretamente com o backlog declarado de “machine-readable real + evidence taxonomy”, então cabe em **Lote 2**.

## 5. Recomendação: pode avançar para Lote 2? (SIM COM CAVEATS)

**SIM COM CAVEATS.** Dá para avançar para Lote 2 porque há progresso real e o núcleo da ambiguidade de lifecycle/PRR melhorou bastante. Mas **não** dá para chamar Lote 1 de “encerrado” nem repetir a frase “nenhuma regressão”. Antes ou no início do Lote 2, eu abriria um micro-patch obrigatório de higiene para: (1) transformar o pseudo-front-matter em front matter real e parseável, (2) eliminar o drift residual de `Status/ACCEPTED/SEALED` no corpo dos docs, (3) consertar a regra de split de WI e sua referência quebrada, e (4) dar um campo real para `AUDIT_PENDING`. Sem isso, o Lote 2 começa em cima de metadado que finge ser estruturado, mas não é.

# Calibração dos cinco pilotos — revisão 1.4

Estado: evidência documental de capacidade; não é aprovação fria, freeze,
prova de runtime ou autorização de publicação.

## Pin e método

- **Bytes medidos:** branch `codex/corelink-ownership-campaign`, commit
  `1f0786f09e094978d75d8c31de54409bb7a6b873`.
- **Readback do repositório:** `origin/main` observado em
  `0389714d9f5408f744e17227b82d795fff245a32` em 2026-09-22. A população Cargo
  continua em 107 manifests/105 pacotes, mas o framework v1.4 não está nessa
  branch. O `corelink-server` teve mudanças materiais no source tree; portanto
  o readback histórico em `140e16e` não é prova de current-main. Hash, cf-bindings
  e e2e não tiveram mudança de implementação no delta comparado; billing teve
  mudança de teste. Os avanços posteriores de `f5c963f2` para `4d5d191c` e
  `0389714d` não tocaram os cinco pilotos. Veja `MAIN-REANCHOR-20260922.md`.
- **Medição:** `wc -l -w -c` nos quatro caminhos finais de cada piloto.
  Populações materiais foram lidas dos próprios B02/B06, R03/R05 e M03;
  quando o documento usa registros agrupados, a diferença é explicitada.
- **Capacidade:** perfil H só é escolhido quando há um gatilho observável do
  standard (>40 relações atômicas, >20 módulos semânticos ou >8 procedimentos).
  `NO` em overflow significa somente que os bytes medidos cabem no cap do
  perfil escolhido; não significa completude semântica.

## Resultado

| Piloto | Perfil declarado / decisão calibrada | População material | SKILL | REFERENCE | BLAST | MAINTENANCE | Overflow | Decisão de capacidade |
|---|---|---|---:|---:|---:|---:|---|---|
| `corelink-hash` | H declarado / 42 relações | 42 relações, 5 arquivos de implementação, 6 procedimentos | 101 / 781 / 6.8 KiB | 396 / 2,911 / 30.6 KiB | 661 / 5,218 / 55.4 KiB | 202 / 1,868 / 15.5 KiB | NO | Perfil alinhado ao gatilho >40; ledger 42/42, peers/consumers e nova review dos bytes H continuam abertos |
| `corelink-billing` | H declarado / 49 declarações de módulos-fonte | 27 relações, 47 arquivos não-test em `src/`, 7 procedimentos | 94 / 659 / 5.0 KiB | 457 / 2,893 / 23.2 KiB | 478 / 3,224 / 27.4 KiB | 262 / 1,802 / 14.3 KiB | NO | H tem census source-backed de 49 declarações (`BILLING-MODULE-CENSUS-20260922.md`); documentação aprovada em revisão fria, sem runtime observado |
| `corelink-server` | H / 55 relações | 55 relações, 31 dependências first-party declaradas, 5 procedimentos | 79 / 483 / 3.6 KiB | 265 / 1,755 / 13.3 KiB | 795 / 6,562 / 55.8 KiB | 204 / 1,511 / 11.4 KiB | NO | H por >40 relações; source current-main divergiu do pin histórico e exige reancoragem |
| `corelink-cf-bindings` | S / 39 relações | 39 relações atômicas, 16 declarações Cargo, 5 procedimentos | 84 / 587 / 4.8 KiB | 186 / 1,400 / 12.6 KiB | 296 / 2,593 / 24.3 KiB | 117 / 911 / 8.3 KiB | NO | S cabe; consumer resolution, execução e wiring de produção continuam desconhecidos |
| `e2e-billing-flow` | S / sem gatilho H | 9 relações semânticas, 2 artefatos de build, 5 procedimentos | 73 / 554 / 4.2 KiB | 151 / 1,204 / 10.2 KiB | 154 / 1,240 / 10.7 KiB | 113 / 1,101 / 9.8 KiB | NO | S; conflito de refund/ordem e execução continuam bloqueios |

Os números de cada artefato estão em ordem **linhas / palavras / bytes**. Os
bytes medidos no commit declarado cabem no perfil escolhido. Hash foi alinhado a H
por suas 42 relações; billing permanece H, com 49 declarações de módulos-fonte
em 47 arquivos não-test, conforme o census source-backed. Cf-bindings permanece S
porque suas 39 relações não atingem o gatilho H observado. Isso
não encerra os gates: qualquer mudança de perfil ou conteúdo semântico invalida reviews,
e o `corelink-cf-bindings` precisa transformar headings agrupados em relações
realmente atômicas antes de aprovação.

## Limites e uso

Esta calibração demonstra que o contrato candidato é mensurável nos cinco
pilotos; a regra S/H foi aplicada ao hash e billing com evidência explícita,
mas a interpretação semântica das 49 declarações de módulos de billing ainda
requer revisão. Ela não
substitui a reconciliação do `main`, a revisão fria independente, a execução
local autorizada, a verificação de consumidores/peers, nem a decisão de freeze.
Qualquer alteração nos bytes, no censo, nos contratos ou no pin de fonte exige
nova medição e invalida a conclusão correspondente; normalização metadata-only
exige apenas o readback escopado definido em CO-1 §9.3.

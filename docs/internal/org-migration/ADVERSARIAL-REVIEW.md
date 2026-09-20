# Revisão adversarial e regressões que bloqueiam

Esta matriz vem dos contraexemplos da issue #1702. É um roteiro de verificação, não aprovação independente nem atestado live. O checker confirma estrutura, não a verdade do operador.

| ID | Tentativa de tornar o registro verde | Resposta exigida |
| --- | --- | --- |
| A01 | Aceitar backup preliminar ou tag posterior ao freeze como prova final. | G08/G11 requerem mesmo FREEZE_SHA e igualdade de refs/assets/LFS/restore; mudança posterior invalida. |
| A02 | Deixar `PROFILE`, owner ou branch implícitos em comando operacional. | Exigir profile explícito, repo/owner IDs e branch/SHA por readback; ausência é UNKNOWN. |
| A03 | Procurar apenas aliases já conhecidos e ignorar `NewOwner/corelink-server`. | Auditar expressão owner/repo genérica, incluindo caminho; owner fora do catálogo fica UNRESOLVED. |
| A04 | Colapsar duas referências da mesma regra numa linha. | Um finding por ocorrência e regra, ID próprio e intervalos individuais. |
| A05 | Calcular coluna por caracteres ou inventar linha por separador Unicode. | Números de linha por LF; offsets exclusivos em bytes UTF-8 dentro do conteúdo da linha. |
| A06 | Omitir uma referência que só aparece no caminho. | Reportar `location:path`, linha null e offsets de bytes do caminho. |
| A07 | Ler blob ilimitado ou bloquear sem limite. | Limites explícitos de árvore/blob/bytes/findings; processo Git com timeout; limite excedido falha fechado e não gera relatório completo. |
| A08 | Chamar evidências de execuções diferentes de um só snapshot atual. | Incluir perfil/regras/ferramenta hashes, timestamp, versões, limites e source SHA. |
| A09 | Reutilizar gate PASS antigo, scope diferente ou prova após expiração. | Checker schema 2 exige hashes vinculados, owner/reviewer, timestamps, validade e ordem temporal. |
| A10 | Substituir UNKNOWN por lista vazia após 403/404/truncamento. | UNKNOWN sempre bloqueia; ausência precisa de prova e N/A limitado a G07/G17. |
| A11 | Pré-aprovar GO para uma janela futura ou antes do freeze final. | G12 exige evidência atual, mesmo scope hash, issued_at posterior a G11 e janela já iniciada. |
| A12 | Chamar criação do kit, PR ou checklist de autorização. | `migration_ready` e `authorization_verified` permanecem false; identidade humana exige autoridade fora do checker. |
| A13 | Considerar 202, redirect, timeout ou alias como transferência concluída. | G13 exige uma request e readback canônico por repo ID, owner ID e caminho. Estado ambíguo não pode repetir mutação. |
| A14 | Exportar JWT, secret, private key, webhook config ou tenant record para provar. | Evidência privada por referência/hash; nomes/scopes apenas; valores e dados de tenant não são exportados. |
| A15 | Usar retry simultâneo, manter source e destination escrevendo ou reativar cron inteiro. | Freeze/drain, fences, zero writer residual e single-writer reconciliation; retomada seletiva e split-brain bloqueia. |
| A16 | Tratar transfer-back como rollback garantido. | Plano exige recuperação forward acessível; retorno é outra decisão com novo namespace/authority check. |
| A17 | Rotacionar token amplo ou dizer que nome do secret prova consumer. | Credencial por provider/scope/consumer; validar novo consumer e revogar antigo sem capturar valor. |
| A18 | Usar PR do kit para fechar issue operacional. | B-374 é OPEN e ligado à #1702; documentação preparatória não satisfaz gates nem fecha a issue. |

## Condições negativas mínimas

- Repo ID incorreto, target source-equal, nome público, destino null, branch/SHA alterado, scope com peer, ou owner ID divergente bloqueiam.
- Hash da evidência malformado, expirada, futura, scope diferente, sem owner/reviewer, ou `reviewed_at < observed_at` bloqueia.
- Qualquer `UNKNOWN`, falta de gate, status duplicado/reordenado, N/A fora G07/G17, G13–G19 antes de request, ou G12 anterior ao G11 bloqueia.
- Fence ausente, writer residual, SHA de restore divergente, manifest parity falso, raw credential/tenant data exportado, recovery dependente do Actions afetado, ou mais de um writer bloqueia.
- Registro consistente não implica que identidade humana/GO é autêntica. Uma pessoa não pode preencher e revisar a mesma gate sob duas personas.

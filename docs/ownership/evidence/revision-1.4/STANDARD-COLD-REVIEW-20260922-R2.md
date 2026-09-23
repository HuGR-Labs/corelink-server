# Cold rereview do `STANDARD.md` — 2026-09-22 (R2)

Veredito: **BLOCKED**. A rerevisão foi feita após o alinhamento do perfil H de
`corelink-hash`; ela confirma a correção estrutural local, mas não autoriza
freeze nem publicação.

- Sessão/agente: `/root/standard_cold_rereview_after_profile`
- SHA do standard: `f0f7c6ac0fe2276ce84548361ef102f25023034540bc03b772aae0525c07aa21`
- Population readback: 105 packages (95 workspace + 10 fuzz) no pin `fb611330`.
- Structural checks: 420/420; isso não equivale a aprovação.
- Registry: 68 integridades PASS e 37 bloqueadas; cold review global ainda
  `UNVERIFIED`; publicação 0.

## Bloqueadores confirmados

- G0: billing ainda usa 47 arquivos-fonte não-test como proxy de módulos
  semânticos H; a calibração também precisa ser re-medida nos bytes atuais.
- G1: não existe freeze/aprovação válida do standard; hash ainda está
  `BLOCKED` na rerevisão fria independente.
- G2: registry/readback divergem do `main` atual; 37 conjuntos de integridade
  continuam bloqueados; os 336 caminhos documentais da branch não estão no
  `main`; deduplicação e preflight não foram concluídos.
- G3: os cinco pilotos não têm quatro vereditos atuais `APPROVE` uniformes;
  não há evidência Cargo/runtime executada no pin atual.

Este registro não promove nenhum artefato e não substitui a revalidação dos
pilotos, do censo/peers ou do backlog antes de eventual freeze.

## Atualização de escopo

Este card permanece histórico para a SHA `f0f7c6ac`. Depois dele, a campanha
fechou a integridade mecânica em 105/105, publicou o census semântico de billing
e adicionou a emenda metadata-only ao candidato atual (`e9b9c8ae`). O veredito
`BLOCKED` continua válido, mas uma nova cold review do standard deve usar a SHA
atual e não repetir os achados já resolvidos como se ainda fossem o estado
observado.

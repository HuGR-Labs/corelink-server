# Desenho de perfil, auditoria e projeções

## Autoridades separadas

O [perfil de exemplo](topology.example.json) declara papéis, IDs e limites. Ele não é consumido por produção e não autoriza ação. Separar `server`, `runners`, `workspaces`, `cli_distribution`, owner do GitHub App, recursos cloud, publicação, identidades OIDC e política criptográfica. Cada projeção registra produtor, consumidor, justificativa, caminho/ref, owner, prova e revisão.

Repo ID reconhece o repositório através da mudança; owner ID identifica a organização; URL apenas localiza. Nenhuma dessas observações autentica por si só uma pessoa ou concede GO. Self-repo em Actions usa contexto validado. Ferramentas externas exigem repo explícito e GET de readback. Guardas privilegiadas preservam evento, ref, proteção, ID e workflow; nunca derivam a política esperada do branch sob teste.

## Auditoria local

`org_migration_audit.py scan --revision <SHA>` resolve uma revisão para commit e lê sua árvore via Git, sem checkout, rede, hooks, working tree, arquivos não rastreados ou alvo de symlink. Reporta SHA de fonte/perfil/regras/ferramenta, instante, versões, limites, blob SHA, caminho, linha LF, spans de bytes UTF-8, hashes de linha e hints de classificação. Não emite conteúdo bruto.

Cada ocorrência tem ID, regra e localização. Ocorrências iguais na mesma linha e regras sobrepostas permanecem distintas. Regex de owner encontra owners fora do catálogo e marca disposição `UNRESOLVED`. Binários, LFS, gitlinks e blobs acima do limite são explícitos; qualquer `skipped_or_partial` torna `coverage_complete:false`. Limites excedidos falham sem relatório completo. Isto é triagem literal, não análise semântica, censo live ou autorização para editar.

`plan` aceita perfil sem destino e continua `PLANNING_ONLY_NOT_AUTHORIZED`; `compare` identifica delta e continua `REVIEW_REQUIRED`. O script não tem comandos apply, transfer, dispatch, rewrite, deploy, publish ou credential rotation.

## Ledger e autoridade

O checker separado lê apenas o [ledger schema 2](gate-ledger.template.json), exige os 20 IDs na ordem, enumera status, responsáveis, revisão, hashes de escopo/evidência, timestamps e expiração. `UNKNOWN`, `BLOCKED`, `FAIL`, evidência ausente, vencida, escopo divergente ou ordem temporal inválida bloqueiam consistência. O checker não lê o artefato apontado, confirma hash externo, autentica assinatura, identifica pessoas, prova verdade, certifica autorização ou define `migration_ready`/`authorization_verified` como true.

`record_consistent:true` atesta apenas formato e coerência declarada. O checker não baixa nem autentica provas. Esses campos continuam false mesmo num registro consistente. A transferência não é automatizada por ferramenta.

## Projeções futuras de runtime

O kit não migra consumidores de produção. WP-02 deve mapear cada consumidor ativo a uma projeção explícita e testar a origem e as topologias transitórias. Diretrizes:

- URL própria vem do contexto GitHub validado; repo alvo remoto requer owner e repo ID readback.
- OIDC compara issuer, audience, owner/repo IDs, workflow, ref e digest contra política independente e versionada.
- Bundles históricos continuam vinculados a refs/digests e policy histórica; novas publicações usam identidade nova. Nunca reescrever assinatura/histórico.
- CLI, peers, registries, App, runner bootstrap, Cloudflare e identidade de produto mantêm autoridade própria. Falta de configuração privilegiada não cai silenciosamente no owner antigo.
- Cada hardcode ativo recebe disposição por ocorrência com SHA, responsável, prova e revisor; exceções amplas por diretório não são aceites.

O perfil futuro deve ser pequeno, estrito, versionado e compatível com a origem. A existência do JSON deste kit não é prevenção de regressão; essa proteção só existe após consumidores, detector de drift e testes serem integrados e provados.

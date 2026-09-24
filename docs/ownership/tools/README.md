# Ferramentas de execução local — CO-1 v1.1

[Pacote](../README.md) · [Contrato comum](../COMMON.md) · [Matriz de garantias](../VALIDATION-MATRIX.md).

Python 3.11+. `check_docs`/`ownership_gate` usam as versões em `requirements.txt`.
`prepare_census`, `render_issue` e `publication_gate` usam a biblioteca padrão.
Nenhuma ferramenta publica no GitHub, modifica produção ou executa comandos lidos
em Markdown. O censo invoca somente comandos Git de leitura e Cargo metadata.

## Preparação

Em um ambiente isolado com as dependências disponíveis:

```sh
python -m pip install -r requirements.txt
python -m unittest discover -s tests -v
python tests/adversarial_probe.py
```

O pip pode exigir rede; esta revisão usou dependências já instaladas, com versões
registradas nas evidências. Os comandos acima não foram executados no Mac do usuário.

## check_docs.py

```sh
python tools/check_docs.py templates/SKILL.md.tmpl --kind skill --template
python tools/check_docs.py templates/REFERENCE.md.tmpl --kind reference --template
python tools/check_docs.py templates/BLAST_RADIUS.md.tmpl --kind blast_radius --template
python tools/check_docs.py templates/MAINTENANCE.md.tmpl --kind maintenance --template
```

Para uma entrega real, retirar `--template`, informar arquivo real e `--root`.
O parser CommonMark distingue exemplos/comentários de seções; usa YAML seguro
com chaves únicas. Mede resumo, parágrafos, código, fichas e os tetos totais;
confere tipo, perfil, naming, índice de registros e links locais explícitos.

O resultado se chama `IMPLEMENTED_CHECKS_PASS`. O checker não certifica conteúdo,
independência, URLs externas, navegação cross-file em três cliques ou adequação
semântica do perfil. Âncoras automáticas antigas precisam de seu renderer real.

## prepare_census.py

Exige checkout Git limpo e baseline explícita. Um diretório que só tem `origin/main`
correto mas está em outra branch NÃO é aceito. Não faz checkout/reset nem instala
Rust. Uma opção `--cargo-bin` aceita executável Cargo configurado, não shell string.

```sh
python tools/prepare_census.py --repo-root /checkout/corelink-server \
  --expected-commit cca798ff5bc2df660ecf2570ed243eb9775ff3d0 \
  --output-dir /tmp/corelink-ownership-census-new
```

Saída nova e fora do repo. Preserva metadados/targets/aliases/features, testa nome
do manifesto e confere HEAD/lock/limpeza antes e depois. Grafo declarado não é
alcance runtime. Arquivos gerados não entram no repositório implicitamente.

Para classificar todos os manifestos próprios fora do workspace, fornecer
`--scope-decisions arquivo.json`. Cada chave é um manifesto rastreado conhecido;
cada valor exige `classification`, `reason`, `evidence`. Classificações:
`first_party_independent`, `third_party_vendor`, `test_fixture`, `archive`. Workspaces sem
package são identificados estruturalmente. Sem decisão, fica UNCLASSIFIED.

Para independentes, invoca metadata locked/offline de seu manifesto e concatena
as populações, recusando nomes/manifests/slugs duplicados e reconstruindo inversas.
Não presume que todos os nomes fuzz são próprios ou que todo vendor é terceiro.
A classificação `archive` é reservada a manifestos históricos rastreados, fora do
workspace e acompanhados de evidência. A flag de escopo completo só sobe quando
não resta manifesto sem classificação.
**Esse fluxo não foi executado contra o checkout real nesta revisão.**

## render_issue.py

Consome package do censo real, seed individual, baseline e contrato imutável.
Exige listas não vazias de fatos, contexto OKF, riscos, comandos e fontes do seed.
O autor/revisor ainda precisam conferir a veracidade desse seed. O renderer
recusa placeholder, naming inconsistente, target ausente e estouro de capacidade.

```sh
python tools/render_issue.py --census /tmp/census/census.json \
  --manifest crates/corelink-billing/Cargo.toml --seed billing-seed.json \
  --contract-url "$FROZEN_CONTRACT_URL" --repository HuGR-dev/corelink-server
```

`FROZEN_CONTRACT_URL` deve vir da publicação real aprovada, com commit SHA, não
`main` nem uma URL inventada. Saída em stdout, estado DRAFT_VALIDATED_STRUCTURE;
não cria arquivo no repo, não publica e não representa READY.

## ownership_gate.py

## generate_registry.py

Gera `docs/ownership/registry.json` e `docs/ownership/index.md` a partir dos
quatro caminhos presentes para cada skill/package. O resultado distingue
`STRUCTURAL PASS`, `UNVERIFIED` de cold review e `NOT_PUBLISHED`; nunca promove
aprovação nem publica issue. O pin informado precisa coincidir com `origin/main`
buscado localmente e com `refs/heads/main` no remoto no momento da geração;
fetch ou pin obsoleto bloqueia ambos os arquivos de saída.

```sh
python tools/generate_registry.py --root /checkout/corelink-server \
  --observed-main <40-hex-main-sha> \
  --json-output docs/ownership/registry.json \
  --markdown-output docs/ownership/index.md
```

```sh
python tools/ownership_gate.py --repo-root /checkout/corelink-server \
  /checkout/corelink-server/docs/ownership/records/corelink-billing.json
```

O JSON é um registro preenchido, não o template. O CLI confere caminho do registro,
fontes no commit ancestral declarado e hashes do checkout. Valida schema, quatro
artefatos/checks, evidências, achados, fontes, novos consumidores, direções/peers,
capacidade e status individuais de procedimentos. Falha é saída 1 ou 2.

Vários registros geram registry ordenado e relações cruzadas conciliadas. Acrescente
`--markdown-index` para a visão Markdown do mesmo conjunto. A integração grava
stdout nos arquivos gerados após gates; autores não editam índices em paralelo.
`EVIDENCE_CONSISTENT` é requisito necessário, não atestado da identidade real do
revisor ou aprovação suficiente para merge. A camada Git do CLI permanece a validar
no repo real; os testes unitários exercitam consistência em fixtures isoladas.

## publication_gate.py

```sh
python tools/publication_gate.py --item item.json --snapshot github-snapshot.json \
  --repository-id 1232040291 --repository HuGR-dev/corelink-server
```

Snapshot precisa vir de coleta autorizada completa, abertas/fechadas; body/IDs
não são inventados. O preflight exige os seis gates com evidência e link imutável
para `docs/ownership/STANDARD.md` no commit publicado em `origin/main` que contém
os bytes atuais do padrão.
Detecta marcadores anteriores e duplicatas; UNCERTAIN não autoriza retry. A função
`record_readback` aceita somente retorno exato de repositório, marcador e body hash.
O módulo NÃO contém POST, criação de issue, autenticação, retry remoto ou merge.

## Garantias e evidências

Ver `../evidence/revision-1.1/tests-final.txt` e `adversarial-results.json`.
Fixtures comprovam o comportamento dos helpers, não contratos de billing, runtime,
revisão independente ou integração GitHub/OKF. As verificações manuais e pendências
estão atribuídas em `VALIDATION-MATRIX.md` e `plans/PREPARATION.md`.

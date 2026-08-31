### Removed

- **`cosign-sign.yml` e toda promessa de assinatura Sigstore/Rekor na doc pública.**
  A lane tinha **0 execuções** (controle: `nightly.yml` devolve linhas na mesma
  consulta). Lida por dentro, não era só trigger-starved: eram **cinco** paredes
  estruturais — dispara em tag `v*` e só existem tags `cli-v*`; empurrava para
  `ghcr.io/HumanGuardrail/*` com o `GITHUB_TOKEN` de um repo em `HuGR-Labs`;
  construía o `Dockerfile` da raiz (o **contêiner**) e o rotulava `corelink-worker`,
  que é um Cloudflare Worker sem imagem OCI; carregava `ZONE_ID_PLACEHOLDER` literal
  no payload de deploy; e seu passo de atestação SLSA era um `echo` declarado como
  stub. Além disso duplicava e contradizia o caminho de deploy real
  (`container-build-push-prod → repin → cf-deploy-prod`).

  Como a lane sustentava promessas ao cliente, elas saíram junto: nove arquivos da
  doc pública deixam de afirmar assinatura que nunca foi emitida — entre eles a
  linha da tabela de criptografia que anunciava uma chave Ed25519 "offline
  HSM-backed" inexistente, a página de preço, e a página de SBOM, que ensinava um
  `cosign verify-blob` contra uma chave `$TBD` nunca publicada. Nenhum vira
  silêncio: todos passam a declarar o estado real.

  Assinatura de release continua sendo o objetivo certo e está rastreada no B-091;
  refazê-la é desenho novo, não a ressurreição deste arquivo.

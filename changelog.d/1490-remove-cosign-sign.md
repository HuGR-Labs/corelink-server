### Removed

- **`cosign-sign.yml` e a promessa específica da lane OCI de assinatura Sigstore/Rekor.**
  A lane tinha **0 execuções** (controle: `nightly.yml` devolve linhas na mesma
  consulta). Lida por dentro, não era só trigger-starved: disparava em tag `v*`
  quando só existem tags `cli-v*`, publicava no repositório errado, construía o
  contêiner incompatível com o Worker e duplicava o caminho de deploy real.

  As promessas públicas específicas dessa lane saíram junto e agora declaram o
  estado real. Os caminhos de assinatura de release-SLSA e CAS continuam
  separados, explicitamente gated e rastreados em B-091/B-112; este change não
  afirma que Sigstore está ausente de todo o produto.

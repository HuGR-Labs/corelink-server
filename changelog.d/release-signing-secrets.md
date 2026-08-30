### Added

- **As credenciais que faltavam para `sign-linux` e `terraform-drift` existem agora.** Chave GPG RSA-4096 **dedicada** a assinar releases do `corelink-cli` (`0xEC0AD89A75EC6756`, expira 2028-08-29), service token do Cloudflare Access, e bucket R2 `corelink-tfstate` com o endpoint S3. Sete secrets registrados na matriz — `code_only=0`, sem drift. A chave é nova de propósito: `BACKUP_GPG_PUBLIC_KEY` é a metade PÚBLICA usada para cifrar backups e a privada correspondente fica offline por desenho, então reusá-la para assinar seria reuso entre finalidades e traria a privada para online.

### Fixed

- **`release-cli` nunca esteve bloqueado por secret.** O único que ele consome, `CORELINK_CLI_RELEASE_TOKEN`, já existia; a falha real é o step `cargo zigbuild`. Como `sign-linux`, `sign-windows` e `notarize-macos` disparam por `workflow_run` atrás dele, nenhuma delas é sequer alcançada — mesmo com todos os certificados na mão, nada seria assinado porque o artefato não é produzido. Corrigido no enquadramento do B-064.

### Fixed

- **Os avisos de privacidade publicados dirigiam titulares a um TLD reservado (B-100).**
  `apps/admin-ui/src/content/privacy-notice.{en,pt,es,de}.{md,ts}` instruíam contato em
  `privacy@corelink.example`, `privacidade@corelink.example`, `privacidad@corelink.example`
  e `dpo@corelink.example`. O TLD `.example` é reservado pela RFC 2606 e não entrega
  correio, de modo que um titular exercendo direito do Art. 15/17 do RGPD — ou um
  regulador em contato inicial — escrevia para o vazio. Os oito arquivos passam, no mesmo
  commit, a `privacy@humangr.com` e `dpo@humangr.com`, os endereços que o `legal/` já
  usa. Saiu junto o endereço postal "350 Mission St, Suite 1200, San Francisco", que não
  era corroborado em nenhum outro ponto do repositório. Nenhum código foi alterado.

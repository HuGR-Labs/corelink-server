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
- **O `verify` do B-100 aceitava a divergência entre idiomas que o item existe para
  impedir.** O portão exigia `ok -gt 0` — um único arquivo correto bastava —, enquanto o
  corpo do item diz que "corrigir só um idioma seria pior que não corrigir nenhum". Dois
  mutantes sobreviviam: 7 dos 8 avisos regredindo para `hugr.dev` com só o `en.ts` certo,
  e apagar o endereço de `pt.ts`+`es.ts`. Passa a exigir `ok = c` (todos os 8), com o
  `grep` do `ok` escopado ao mesmo conjunto `privacy-notice.*` que o `c` conta. Ambos os
  mutantes agora ficam DRIFTED.

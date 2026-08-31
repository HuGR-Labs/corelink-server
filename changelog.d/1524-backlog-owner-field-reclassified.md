### Fixed

- **O campo `owner:` do `BACKLOG.md` marcava "cheiro de decisão" em vez de bloqueio real, e
  com isso tirou 14 itens de engenharia da fila.** O campo carregava dois significados ao
  mesmo tempo — *"está bloqueado nele agora"* e *"foi decisão dele um dia"* — e um campo com
  dois significados não decide nada quando alguém o lê. A consequência não foi cosmética:
  itens de engenharia comum ficaram parados esperando uma pessoa que não sabia que estava
  sendo esperada. As linhas `owner: owner` caíram de **26 para 12**, e a regra passou a estar
  escrita em `## Rules`, no presente: `owner: owner` só quando o próximo passo é fisicamente
  impossível sem ele — a credencial dele, o dinheiro dele, a assinatura dele, a máquina dele.
  Investigar, medir, escrever o conserto e deixar o PR pronto nunca conta, **nem quando a
  última tecla é dele**. Cada item que ficou passou a **nomear o ato** que só ele executa
  (clicar em "disable" no painel Stripe, assinar o aditivo, comprar o certificado
  Authenticode); cada item que saiu passou a nomear o **próximo passo concreto** e por que ele
  é `tl`.
- **`B-046` desceu para `tl` pelo critério do próprio item.** O corpo já dizia que o bloqueio
  é *da plataforma* (o R2 devolve `NotImplemented` para Object Lock), *"not on an owner infra
  decision"*, e o `verify-means` prescreve **re-sondar o R2** — que é medição. A única saída
  que gasta dinheiro está condicionada a um contrato de cliente que ainda não existe: decisão
  a jusante de uma decisão a jusante, que é exatamente a classe que esta passada declara não
  ser fundamento. O item registra o ato que o faria voltar a `owner:` — assinar a compra do
  segundo backend — para o dia em que o contrato existir. A regra em `## Rules` ganhou a
  cláusula correspondente.
- **`B-160` reescrito: o bloqueio de owner acabou, a lacuna de produto não.** Com
  `CORELINK_PAT_MINT_AUTH_KEY` autorizada, cunhar os dois PATs deixou de depender dele e o
  item desce para `tl`. O que ele passa a rastrear é só a lacuna real, que a autorização não
  toca: **`POST /v1/pats` não está montado em servidor nenhum** e a única rota publicada é um
  wizard de sign-up com senha. Fica registrado, no item e no `verify-means`, que a chave de
  mint é credencial de **operador**: medir por ela mede **permissão**, não a rota que o
  cliente tem, e toda medição feita por ali deve declará-lo.
- **A prosa foi reconciliada com o campo, nos dois sentidos.** Cinco itens (`B-071`, `B-083`,
  `B-094`, `B-096`, `B-105`) traziam *"Owner, não tl:"* ou *"Owner: material comercial"* sob um
  campo que agora é `tl`; deixá-las de pé apenas trocaria "o campo mente" por "a prosa mente".
  Cada uma agora separa explicitamente a **decisão a jusante**, que é do owner, do **próximo
  passo**, que é medir e deixar pronto. A varredura inversa rodou sobre os 167 itens e achou
  um único caso — `B-026`, cujo *"the open decision … is not mine"* ficou stale no dia em que
  o owner tomou a decisão (2026-08-24); a frase foi datada, não apagada. `B-156` também foi
  corrigido: citava `B-083`, `B-088` e `B-094` como *"já são `owner:`"*, o que este mesmo
  commit torna falso.
- **`## Needs the owner` passa a declarar que é agrupamento histórico, não classificação.**
  Nenhum portão lê o cabeçalho, e realocar blocos produziria um diff que conflita com todo PR
  irmão sem comprar nada — então os itens ficam onde estão e o cabeçalho diz, explicitamente,
  que a autoridade é o campo dentro de cada bloco.

Nenhum `status` foi tocado: o campo `owner:` e a prosa, apenas.

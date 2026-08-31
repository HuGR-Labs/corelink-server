### Fixed

- **O campo `owner:` do `BACKLOG.md` marcava "cheiro de decisão" em vez de bloqueio real, e
  com isso tirou 18 itens de engenharia da fila.** O owner disse, textualmente, que a maioria
  dos itens marcados como dele são meus e que ele não é necessário para eles — e a medição
  confirma. O campo carregava dois significados ao mesmo tempo, *"está bloqueado nele agora"* e
  *"foi decisão dele um dia"*, e um campo com dois significados não decide nada quando alguém o
  lê. A consequência não foi cosmética: itens de engenharia comum ficaram parados esperando uma
  pessoa que não sabia que estava sendo esperada. As linhas `owner: owner` caíram de **31 para
  13**, e a regra passou a estar escrita em `## Rules`, no presente: `owner: owner` só quando o
  próximo passo é fisicamente impossível sem ele — a credencial dele, o dinheiro dele, a
  assinatura dele, a máquina dele. Investigar, medir, escrever o conserto e deixar o PR pronto
  nunca conta, **nem quando a última tecla é dele**; "é decisão de produto", "é material
  comercial" e "é risco/conformidade" também não, porque a preparação é minha e a decisão a
  jusante não bloqueia o item.
- **Cinco dos 18 eram contradição na cara: `status: done` com `owner: owner`.** Um item pronto
  não pode continuar bloqueado em ninguém. Em `B-031`, `B-041` e `B-042` a decisão do owner
  existe, mas é **procedência datada** e vive no corpo — o campo carrega estado, não história.
  Em `B-043` e `B-085` não havia decisão de owner alguma a registrar: uma varredura do corpo
  inteiro devolve prova técnica e nada mais, ou seja o campo nunca foi procedência, foi rótulo
  errado. Isso torna concreto o defeito que o `B-147` descreve — ele afirmava que *nenhum* item
  `done` carregava o campo, o que deixou de ser verdade sem que nada apitasse, porque o `verify`
  dele mede a ausência do portão e não a contagem viva. A afirmação foi corrigida com data; o
  portão continua sendo trabalho do próprio `B-147`.
- **`B-160` reescrito: o bloqueio de owner acabou, a lacuna de produto não.** O owner autorizou
  o uso de `CORELINK_PAT_MINT_AUTH_KEY`, então cunhar os dois PATs de tenants distintos deixou
  de depender dele e o item desce para `tl`. O que ele passa a rastrear é só a lacuna real, que
  a autorização não toca: **não existe caminho self-service para o cliente obter um PAT** —
  `POST /v1/pats` não está montado em servidor nenhum e a única rota publicada é um wizard de
  sign-up com senha. Fica registrado no item que a chave de mint é credencial de **operador**:
  medir por ela mede permissão, não a rota que o cliente tem, e toda medição feita por ali deve
  declará-lo.
- **A prosa foi reconciliada com o campo, nos dois sentidos.** Cinco itens (`B-071`, `B-083`,
  `B-094`, `B-096`, `B-105`) traziam *"Owner, não tl:"* ou *"Owner: material comercial"* sob um
  campo que agora é `tl`; deixá-las de pé apenas trocaria "o campo mente" por "a prosa mente".
  Cada uma agora separa explicitamente a **decisão a jusante**, que é do owner, do **próximo
  passo**, que é medir e deixar pronto. A varredura inversa — itens `tl` cuja prosa pede o owner
  — rodou sobre os 166 itens e não achou nenhum que devesse subir. `B-156` também foi corrigido:
  ele citava `B-083`, `B-088` e `B-094` como *"já são `owner:`"*, o que este mesmo commit torna
  falso, e agora aponta para os instrumentos assinados que de fato seguem dele.
- **`## Needs the owner` passa a declarar que é agrupamento histórico, não classificação.**
  Medido: 20 dos 28 itens sob o cabeçalho já carregavam `owner: tl`. Nenhum portão lê o
  cabeçalho, e realocar blocos produziria um diff que conflita com todo PR irmão sem comprar
  nada — então os itens ficam onde estão e o cabeçalho diz, explicitamente, que a autoridade é
  o campo dentro de cada bloco.

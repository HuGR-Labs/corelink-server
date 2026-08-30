### Fixed

- **A `main` reprovava o próprio portão OKF: uma âncora de blob apontava um objeto que nunca esteve nesta história.** O #1431 ancorou `limiter.rs` no blob de um commit intermediário do PR; o squash-merge substituiu esse commit e a âncora ficou pendurada. No PR estava verde e corretamente verde — o commit intermediário ainda existia no branch. A lição é operacional e corrige uma recomendação que eu vinha dando: âncora de blob é robusta a **rebase**, não é imune a **squash** se for tirada de um commit intermediário; tem de ser reconferida contra a `main` depois do merge.

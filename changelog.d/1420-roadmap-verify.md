### Fixed

- **O roadmap de remediação passa a ter `verify` próprio (B-061).** Era o único artefato operacional do repo cuja deriva ninguém detectava — e três números publicados nele não reproduziam, um dos quais já havia vazado para o corpo de um PR mergeado. O `verify` re-deriva a contagem de âncoras OKF inalcançáveis e falha nos dois sentidos: se alguém reancorar sem atualizar o texto, ou se o apodrecimento continuar e o texto subestimar.

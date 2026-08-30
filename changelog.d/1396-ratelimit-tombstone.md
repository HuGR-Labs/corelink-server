### Fixed

- **Um bucket de rate-limit drenado podia ser contornado pela própria evicção.** Quando o bucket esgotado era removido do mapa por pressão de memória, a requisição seguinte recriava-o cheio: o cliente recuperava a cota inteira sem esperar a janela. O teste `eviction_reset_bypass` reproduz o bypass antes do fix. A correção deixa um tombstone no lugar do bucket evictado, de modo que a recriação herda o estado drenado em vez de partir do zero.

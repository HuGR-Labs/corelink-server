### Fixed

- **O manifesto do re-corte do devenv tinha duas instruções que produziriam resultado quebrado.** A linha do `wrangler.toml` mandava pegar só o hunk top-level do binding do DO — mas `durable_objects` não é herdável, a main espelha 6↔6, e o `RUNNER_DEVENV_DO` é o único não espelhado: a feature ficaria verde em dev e 503 permanente em prod, com link visível pro cliente. A linha da migration omitia que a tabela é tenant-keyed e reprovaria o gate que a main instalou depois que essa mesma tabela partiu um apagamento GDPR ao meio.

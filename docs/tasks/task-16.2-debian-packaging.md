# Task 16.2 — Distribuição nativa Debian (avaliação) 🔲

**Phase:** 16 (Observabilidade / Distribuição)

## Contexto

Pergunta do Marcio (01/08/2026): qual a dificuldade de trocar Docker por uma
imagem Debian personalizada?

## Avaliação

Dois níveis, esforços muito diferentes:

### Nível 1 — Pacotes .deb + systemd (RECOMENDADO, ~2 dias)
- `cargo-deb` empacota o collector-core (binário único) com unit systemd,
  usuário de serviço, /etc/flow-collector/, postinst criando diretórios
- Dashboard: dist/ estático servido pelo nginx do sistema (conf incluída no .deb)
- ClickHouse: dependência do repositório apt oficial (deb.clickhouse.com)
- ExaBGP: pacote Debian existente (python3-exabgp) ou pip no postinst
- Ollama: opcional, script oficial
- Um meta-pacote `flowvision` amarra tudo; `install.sh` vira `apt install`
- Requisito técnico: trocar native-tls → rustls no reqwest para eliminar a
  dependência de versão do libssl do sistema

### Nível 2 — Imagem/appliance Debian completa (~1-2 semanas)
- ISO com preseed ou imagem cloud (FAI/debos): instala Debian mínimo + os .deb
  do nível 1 + hardening; vira "appliance FlowVision"
- Faz sentido quando houver escala de instalação em hardware do cliente

## Decisão pendente

Nível 1 pode começar quando quiser; nível 2 só com demanda comercial real.

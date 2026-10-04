# Task 17.11 — Sessão expirada leva ao login

**Phase:** 17 (Drill-down por IP)
**Files:** `frontend/src/lib/sessionGuard.ts` (NEW), `frontend/src/main.ts`,
`frontend/src/stores/auth.ts`, `frontend/src/router.ts`, `frontend/src/views/Login.vue`

## Problema

Relato do Marcio (04/10/2026): a tela de Exporters mostrou "Failed to load exporters" na
homologação. A API estava saudável — o token de login (24 h, `TOKEN_EXPIRATION_HOURS`)
tinha expirado e cada tela tratava o 401 como erro genérico ou ficava vazia, sem levar o
usuário de volta ao login.

## Regras

- **R-01** — Qualquer resposta **401** de `/api/*` (exceto `/api/auth/login`) encerra a
  sessão local (token e usuário) e leva ao login. Várias respostas 401 simultâneas
  disparam um único redirecionamento.
- **R-02** — Antes de cada navegação para rota autenticada, o router confere o `exp` do
  token (JWT). Expirado → mesmo fluxo do R-01, sem esperar uma chamada falhar.
- **R-03** — O login mostra "Sua sessão expirou. Entre novamente." quando chega por
  expiração e, após entrar, volta para a página em que o usuário estava (`redirect`).
  `redirect` só é aceito se for um caminho interno (começa com `/`, não com `//`);
  senão vai para o dashboard.

## Aceite

- AC-01: token expirado + abrir Exporters → tela de login com o aviso; após entrar, volta
  para Exporters.
- AC-02: senha errada no login continua mostrando "Usuário ou senha inválidos" (o 401 do
  próprio login não redireciona).
- AC-03: `redirect=//evil.com` → após entrar, vai para `/dashboard`.

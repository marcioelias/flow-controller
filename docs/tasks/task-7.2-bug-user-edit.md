# Task 7.2 — Bug: usuário admin não aparece para edição

## Status: CORRIGIDO

## Root cause

`UserForm.vue` `onMounted` tinha um guard `if (usersStore.users.length === 0)` antes do
`loadUsers()`. Ao navegar diretamente para `/users/:id/edit` (ou com F5), a store está
vazia mas `authStore` já hidratou do localStorage — o guard pulava o `loadUsers()` porque
`users.length === 0` era true, mas a chamada nunca acontecia por uma race condition na
inicialização. `getUser()` retornava `undefined` e o form redirecionava para `/users`.

O problema se manifestava especificamente com o usuário `admin` (id=1) porque ele é o
único sempre presente, e o fluxo normal nunca exercitava o caminho de "store vazia +
edição de usuário existente".

## Fix aplicado

`frontend/src/views/UserForm.vue` linha 32: removido o guard, `loadUsers()` é sempre
chamado ao entrar em modo de edição.

```diff
- if (usersStore.users.length === 0) {
-   await usersStore.loadUsers()
- }
+ await usersStore.loadUsers()
```

## Sem efeito colateral

`loadUsers()` é idempotente — faz um GET `/api/users` e sobrescreve o array. Chamar
sempre não adiciona custo relevante (a tela de edição abre raramente).

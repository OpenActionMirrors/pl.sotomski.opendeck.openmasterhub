# MasterHub surface for Companion

Companion surface plugin for a MasterHub. It talks to `masterhubd` over the local WebSocket API. See [docs/companion.md](../../docs/companion.md).

```sh
pnpm install
pnpm --filter "companion-surface-masterhub..." build
```

`pnpm --filter companion-surface-masterhub dev` recompiles while Companion is pointed at this directory.

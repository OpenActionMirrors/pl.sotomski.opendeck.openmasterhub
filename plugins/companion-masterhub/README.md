# MasterHub connection for Companion

Companion connection module for MasterHub LED color, mode, and brightness. It talks to `masterhubd`. See [docs/companion.md](../../docs/companion.md) and [HELP.md](./companion/HELP.md).

## Getting started

From the repository root, install dependencies and build this module:

```sh
pnpm install
pnpm --filter "companion-module-masterhub..." build
```

That build is enough for Companion to load the module. `pnpm --filter companion-module-masterhub dev` recompiles while Companion is pointed at this directory.

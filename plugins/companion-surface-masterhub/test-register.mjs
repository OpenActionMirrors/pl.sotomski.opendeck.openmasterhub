// eslint-disable-next-line n/no-unsupported-features/node-builtins -- Node's loader API
import { register } from 'node:module'
import { pathToFileURL } from 'node:url'

register('./test-hooks.mjs', pathToFileURL('./'))

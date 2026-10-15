/* eslint-disable @typescript-eslint/no-require-imports -- companion-surface-build loads this file with require */
const fs = require('node:fs')
const path = require('node:path')

const release = path.join(__dirname, '../../daemons/target/release/masterhubd')
const debug = path.join(__dirname, '../../daemons/target/debug/masterhubd')
const binary = fs.existsSync(release) ? release : debug

module.exports = {
	extraFiles: fs.existsSync(binary) ? [binary] : [],
}

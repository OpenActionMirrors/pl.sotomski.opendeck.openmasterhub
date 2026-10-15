import assert from 'node:assert/strict'
import { test } from 'node:test'

import { encodeRgbJpeg, MAX_JPEG_BYTES } from './jpeg.js'

void test('a dial-sized frame stays inside the daemon JPEG limit', () => {
	const width = 240
	const height = 240
	const rgb = new Uint8Array(width * height * 3)
	for (let index = 0; index < rgb.length; index += 1) {
		rgb[index] = (index * 17) & 0xff
	}
	const encoded = encodeRgbJpeg(rgb, width, height)
	assert.equal(encoded[0], 0xff)
	assert.equal(encoded[1], 0xd8)
	assert.ok(encoded.length <= MAX_JPEG_BYTES)
})

void test('a key-sized frame is a JPEG', () => {
	const encoded = encodeRgbJpeg(new Uint8Array(72 * 72 * 3), 72, 72)
	assert.equal(encoded[0], 0xff)
	assert.equal(encoded[1], 0xd8)
	assert.ok(encoded.length <= MAX_JPEG_BYTES)
})

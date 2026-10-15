import { encode as encodeJpeg } from 'jpeg-js'

/** masterhubd rejects a JPEG larger than this. */
export const MAX_JPEG_BYTES = 64 * 1024

const blackFrames = new Map<string, Buffer>()

export function encodeRgbJpeg(rgb: Uint8Array, width: number, height: number): Buffer {
	const pixels = width * height
	if (rgb.length < pixels * 3) {
		throw new Error(`expected ${pixels * 3} RGB bytes, got ${rgb.length}`)
	}
	const rgba = Buffer.alloc(pixels * 4)
	for (let pixel = 0; pixel < pixels; pixel += 1) {
		rgba[pixel * 4] = rgb[pixel * 3] ?? 0
		rgba[pixel * 4 + 1] = rgb[pixel * 3 + 1] ?? 0
		rgba[pixel * 4 + 2] = rgb[pixel * 3 + 2] ?? 0
		rgba[pixel * 4 + 3] = 255
	}
	let quality = 80
	let encoded = encodeJpeg({ data: rgba, width, height }, quality)
	while (encoded.data.length > MAX_JPEG_BYTES && quality > 10) {
		quality -= 10
		encoded = encodeJpeg({ data: rgba, width, height }, quality)
	}
	if (encoded.data.length > MAX_JPEG_BYTES) {
		throw new Error(`JPEG is ${encoded.data.length} bytes, above the 64 KiB limit`)
	}
	return encoded.data
}

export function blackJpeg(width: number, height: number): Buffer {
	const key = `${width}x${height}`
	const cached = blackFrames.get(key)
	if (cached) {
		return cached
	}
	const encoded = encodeRgbJpeg(new Uint8Array(width * height * 3), width, height)
	blackFrames.set(key, encoded)
	return encoded
}

declare module 'jpeg-js' {
	export function encode(
		image: { data: Buffer; width: number; height: number },
		quality?: number,
	): { data: Buffer; width: number; height: number }
}

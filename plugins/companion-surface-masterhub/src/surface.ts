import type {
	CardGenerator,
	SurfaceContext,
	SurfaceDrawProps,
	SurfaceInputVariable,
	SurfaceInstance,
	SurfacePincodeMap,
	SurfaceRegisterProps,
} from '@companion-surface/base'
import type { MasterHubClient } from '@openmasterhub/sdk'
import { mapDaemonEvent, SliderMemory, type DaemonEvent } from './input.js'
import { blackJpeg, encodeRgbJpeg } from './jpeg.js'
import type { GridControl, SurfaceGrid } from './layout.js'

export interface PaintClient {
	readonly client: MasterHubClient | null
}

const KEY_SIZE = 72
const DIAL_SIZE = 240

export class MasterHubSurface implements SurfaceInstance {
	readonly productName = 'MasterHub'
	private readonly sliders = new SliderMemory()
	private readonly controlIds: ReadonlySet<string>

	constructor(
		readonly surfaceId: string,
		private readonly grid: SurfaceGrid,
		private readonly context: SurfaceContext,
		private readonly link: PaintClient,
		private readonly onClose: () => void,
	) {
		this.controlIds = new Set(grid.controls.map((control) => control.id))
	}

	async init(): Promise<void> {}

	async ready(): Promise<void> {}

	async close(): Promise<void> {
		this.onClose()
	}

	async setBrightness(percent: number): Promise<void> {
		const brightness = Math.min(100, Math.max(0, Math.round(percent)))
		await this.link.client?.command('set_backlight', {
			module: { model: 'M4315', id: 'keys' },
			brightness,
		})
	}

	async blank(): Promise<void> {
		await Promise.all(
			this.grid.controls.map(async (control) => {
				const size = bitmapSize(control)
				if (!size) {
					return
				}
				await this.sendJpeg(control, blackJpeg(size, size))
			}),
		)
	}

	async draw(signal: AbortSignal, drawProps: SurfaceDrawProps): Promise<void> {
		if (signal.aborted || !drawProps.image) {
			return
		}
		const control = this.grid.controls.find((item) => item.id === drawProps.controlId)
		const size = control ? bitmapSize(control) : null
		if (!control || !size || drawProps.image.length < size * size * 3) {
			return
		}
		const encoded = encodeRgbJpeg(drawProps.image, size, size)
		if (signal.aborted) {
			return
		}
		await this.sendJpeg(control, encoded)
	}

	async showStatus(signal: AbortSignal, cardGenerator: CardGenerator, _statusMessage: string): Promise<void> {
		if (signal.aborted) {
			return
		}
		const control = this.grid.controls.find((item) => bitmapSize(item) !== null)
		const size = control ? bitmapSize(control) : null
		if (!control || !size) {
			return
		}
		const image = await cardGenerator.generateBasicCard(size, size, 'rgb')
		if (signal.aborted) {
			return
		}
		await this.draw(signal, { controlId: control.id, image, pressed: false })
	}

	showLockedStatus(locked: boolean, _characterCount: number): void {
		if (locked) {
			void this.blank()
		}
	}

	disconnect(error: Error): void {
		this.context.disconnect(error)
	}

	handle(event: DaemonEvent): void {
		const mapped = mapDaemonEvent(event, this.sliders, this.controlIds)
		if (!mapped) {
			return
		}
		if (mapped.pressed === true) {
			this.context.keyDownById(mapped.controlId)
		} else if (mapped.pressed === false) {
			this.context.keyUpById(mapped.controlId)
		}
		if (mapped.delta) {
			this.context.rotateById(mapped.controlId, mapped.delta)
		}
		if (mapped.slider !== undefined && mapped.controlId.startsWith('slider/')) {
			this.context.sendVariableValue(`slider_${mapped.controlId.slice('slider/'.length)}`, mapped.slider)
		}
	}

	private async sendJpeg(control: GridControl, jpeg: Buffer): Promise<void> {
		const client = this.link.client
		if (!client) {
			return
		}
		const data_base64 = jpeg.toString('base64')
		if (control.id === 'dial') {
			await client.command('set_screen_image', {
				module: { model: 'M0064', id: 'base' },
				data_base64,
			})
			return
		}
		if (!control.id.startsWith('key/')) {
			return
		}
		await client.command('set_button_image', {
			module: { model: 'M4315', id: 'keys' },
			control: Number(control.id.slice('key/'.length)),
			data_base64,
		})
	}
}

export function registerProps(grid: SurfaceGrid): SurfaceRegisterProps {
	const transferVariables: SurfaceInputVariable[] = grid.hasSliders
		? [0, 1, 2, 3, 4].map((index) => ({
				id: `slider_${index}`,
				type: 'input' as const,
				name: `Slider ${index + 1}`,
				description: 'Slider position from 0 to 100',
			}))
		: []
	return {
		brightness: true,
		surfaceLayout: {
			stylePresets: {
				default: {},
				key: { bitmap: { w: KEY_SIZE, h: KEY_SIZE, format: 'rgb' } },
				dial: { bitmap: { w: DIAL_SIZE, h: DIAL_SIZE, format: 'rgb' } },
				encoder: {},
			},
			controls: Object.fromEntries(
				grid.controls.map((control) => [
					control.id,
					{ row: control.row, column: control.column, stylePreset: control.stylePreset },
				]),
			),
		},
		transferVariables,
		pincodeMap: pincodeMap(grid),
		location: null,
		configFields: null,
	}
}

function pincodeMap(grid: SurfaceGrid): SurfacePincodeMap | null {
	if (!grid.hasKeypad) {
		return null
	}
	return {
		type: 'single-page',
		pincode: null,
		0: 'key/0',
		1: 'key/1',
		2: 'key/2',
		3: 'key/3',
		4: 'key/4',
		5: 'key/5',
		6: 'key/6',
		7: 'key/7',
		8: 'key/8',
		9: 'key/9',
	}
}

function bitmapSize(control: GridControl): number | null {
	if (control.stylePreset === 'key') {
		return KEY_SIZE
	}
	if (control.stylePreset === 'dial') {
		return DIAL_SIZE
	}
	return null
}

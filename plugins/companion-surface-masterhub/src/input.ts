export interface DaemonEvent {
	action: string
	payload: unknown
}

export interface MappedInput {
	controlId: string
	pressed?: boolean
	delta?: number
	slider?: number
}

export class SliderMemory {
	private readonly values = new Map<number, number>()

	/** The first sample only records the position, so connecting does not turn the encoder. */
	delta(control: number, value: number): number | null {
		const previous = this.values.get(control)
		this.values.set(control, value)
		if (previous === undefined || previous === value) {
			return null
		}
		return value - previous
	}

	clear(): void {
		this.values.clear()
	}
}

export function mapDaemonEvent(
	event: DaemonEvent,
	sliders: SliderMemory,
	controls: ReadonlySet<string>,
): MappedInput | null {
	const payload = objectOf(event.payload)
	const module = objectOf(payload?.module)
	const model = stringOf(module?.model)
	const id = stringOf(module?.id)
	const control = numberOf(payload?.control)
	if (!model || !id || control === null) {
		return null
	}

	if (event.action === 'key_state' && model === 'M4315' && id === 'keys' && control >= 0 && control <= 14) {
		return press(`key/${control}`, payload?.pressed, controls)
	}
	if (event.action === 'knob_state' && model === 'M003K' && id === 'knobs' && control >= 0 && control <= 2) {
		return press(`knob/${control}`, payload?.pressed, controls)
	}
	if (event.action === 'knob_state' && model === 'M0064' && id === 'base' && control === 0) {
		return press('dial', payload?.pressed, controls)
	}
	if (event.action === 'knob_rotated' && model === 'M003K' && id === 'knobs' && control >= 0 && control <= 2) {
		return rotate(`knob/${control}`, detent(payload?.direction, 'cw', 'ccw'), controls)
	}
	if (event.action === 'knob_rotated' && model === 'M0064' && id === 'base' && control === 0) {
		return rotate('dial', detent(payload?.direction, 'cw', 'ccw'), controls)
	}
	if (event.action === 'roller_rotated' && model === 'M032LE' && id === 'rollers' && control >= 0 && control <= 1) {
		return rotate(`roller/${control}`, detent(payload?.direction, 'up', 'down'), controls)
	}
	if (event.action === 'slider_moved' && model === 'M005S' && id === 'faders' && control >= 0 && control <= 4) {
		const value = numberOf(payload?.value)
		const controlId = `slider/${control}`
		if (value === null || value < 0 || value > 100 || !controls.has(controlId)) {
			return null
		}
		const delta = sliders.delta(control, value)
		return delta === null ? { controlId, slider: value } : { controlId, slider: value, delta }
	}
	return null
}

function press(controlId: string, pressed: unknown, controls: ReadonlySet<string>): MappedInput | null {
	if (typeof pressed !== 'boolean' || !controls.has(controlId)) {
		return null
	}
	return { controlId, pressed }
}

function rotate(controlId: string, delta: number | null, controls: ReadonlySet<string>): MappedInput | null {
	if (delta === null || !controls.has(controlId)) {
		return null
	}
	return { controlId, delta }
}

function detent(direction: unknown, positive: string, negative: string): number | null {
	if (direction === positive) {
		return 1
	}
	if (direction === negative) {
		return -1
	}
	return null
}

function objectOf(value: unknown): Record<string, unknown> | null {
	if (typeof value !== 'object' || value === null || Array.isArray(value)) {
		return null
	}
	return value as Record<string, unknown>
}

function stringOf(value: unknown): string | null {
	return typeof value === 'string' ? value : null
}

function numberOf(value: unknown): number | null {
	return typeof value === 'number' && Number.isInteger(value) ? value : null
}

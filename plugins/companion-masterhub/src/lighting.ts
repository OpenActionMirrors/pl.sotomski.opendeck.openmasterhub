export const LED_TARGETS = ['top', 'left', 'right', 'base'] as const

export type LedTarget = (typeof LED_TARGETS)[number]

export const LIGHTING_MODES = [
	'disable',
	'static',
	'breathing',
	'strobe',
	'spring',
	'cycling',
	'random',
	'wave',
	'water',
	'rainbow',
] as const

export type LightingMode = (typeof LIGHTING_MODES)[number]

export const CYCLE_MODES = [
	'static',
	'breathing',
	'strobe',
	'spring',
	'cycling',
	'random',
	'wave',
	'water',
	'rainbow',
] as const

export type CycleMode = (typeof CYCLE_MODES)[number]

export interface LedCommand {
	module: { model: 'M003K'; id: 'knobs' } | { model: 'M0064'; id: 'base' }
	control: number | 'all'
}

const KNOB_INDEX = { top: 0, left: 1, right: 2 } as const

/** One `all` command when every knob is selected. The base is its own command. */
export function ledCommands(targets: readonly LedTarget[]): LedCommand[] {
	const knobs = (['top', 'left', 'right'] as const).filter((target) => targets.includes(target))
	const commands: LedCommand[] = []
	if (knobs.length === 3) {
		commands.push({ module: { model: 'M003K', id: 'knobs' }, control: 'all' })
	} else {
		for (const knob of knobs) {
			commands.push({ module: { model: 'M003K', id: 'knobs' }, control: KNOB_INDEX[knob] })
		}
	}
	if (targets.includes('base')) {
		commands.push({ module: { model: 'M0064', id: 'base' }, control: 0 })
	}
	return commands
}

export function selectedTargets(flags: { base: boolean; top: boolean; left: boolean; right: boolean }): LedTarget[] {
	const targets: LedTarget[] = []
	if (flags.top) {
		targets.push('top')
	}
	if (flags.left) {
		targets.push('left')
	}
	if (flags.right) {
		targets.push('right')
	}
	if (flags.base) {
		targets.push('base')
	}
	return targets
}

/** First press turns the lights solid. The next press turns them off. */
export function nextToggle(current: 'disable' | 'static' | undefined): 'disable' | 'static' {
	return current === 'static' ? 'disable' : 'static'
}

/** First press sends solid. Later presses step through the effects and skip off. */
export function nextCycle(current: string | undefined): CycleMode {
	const index = CYCLE_MODES.indexOf(current as CycleMode)
	if (index < 0) {
		return 'static'
	}
	return CYCLE_MODES[(index + 1) % CYCLE_MODES.length] ?? 'static'
}

export function nextBrightness(current: number, action: 'set' | 'increase' | 'decrease', amount: number): number {
	const next = action === 'set' ? amount : current + (action === 'decrease' ? -amount : amount)
	return Math.min(100, Math.max(0, Math.round(next)))
}

/** Companion color numbers are `ttrrggbb`. The low 24 bits are the RGB channels. */
export function rgbFromCompanionColor(color: number): { r: number; g: number; b: number } {
	return {
		r: (color >> 16) & 0xff,
		g: (color >> 8) & 0xff,
		b: color & 0xff,
	}
}

/** `base` is connected when the daemon lists the base or the dial. They share the M0064 knob. */
export function moduleIsConnected(modules: readonly string[], choice: string): boolean {
	if (choice === 'base') {
		return modules.includes('M0064') || modules.includes('1xDial')
	}
	return modules.includes(choice)
}

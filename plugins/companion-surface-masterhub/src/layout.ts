/**
 * Companion cells, one control each. Site rectangles come from `placement()` in
 * plugins/opendeck-masterhub/src/map.rs. A keypad is 5 columns wide and its site
 * is 4, so blocks can overlap; those reports use the stacked grid instead.
 */

export interface Placement {
	horizontal: boolean
	x: number
	y: number
	w: number
	h: number
}

export interface GridControl {
	id: string
	row: number
	column: number
	stylePreset: 'key' | 'dial' | 'encoder'
}

export interface SurfaceGrid {
	controls: GridControl[]
	hasKeypad: boolean
	hasSliders: boolean
	hasDial: boolean
}

const VISIBLE = ['M4315', 'M003K', 'M032LE', 'M005S', '1xDial'] as const

type VisibleModule = (typeof VISIBLE)[number]

export function placement(module: string, site: string): Placement | null {
	const place = (horizontal: boolean, x: number, y: number, w: number, h: number): Placement => ({
		horizontal,
		x,
		y,
		w,
		h,
	})
	switch (`${module}:${site}`) {
		case 'M003K:124':
		case '1xDial:124':
			return place(true, 0, 0, 2, 2)
		case 'M003K:111':
		case '1xDial:111':
			return place(true, 2, 0, 2, 2)
		case 'M003K:143':
		case '1xDial:143':
			return place(true, 4, 0, 2, 2)
		case 'M003K:44':
		case '1xDial:44':
			return place(true, 0, 2, 2, 2)
		case 'M003K:43':
		case '1xDial:43':
			return place(true, 1, 2, 2, 2)
		case 'M003K:23':
		case '1xDial:23':
			return place(true, 2, 2, 2, 2)
		case 'M003K:112':
		case '1xDial:112':
			return place(true, 3, 2, 2, 2)
		case 'M003K:134':
		case '1xDial:134':
			return place(true, 4, 2, 2, 2)
		case 'M003K:34':
		case '1xDial:34':
			return place(false, 0, 0, 2, 2)
		case 'M003K:113':
		case '1xDial:113':
			return place(false, 2, 0, 2, 2)
		case 'M003K:31':
		case '1xDial:31':
			return place(false, 0, 2, 2, 2)
		case 'M003K:42':
		case '1xDial:42':
			return place(false, 2, 2, 2, 2)
		case 'M003K:123':
		case '1xDial:123':
			return place(false, 0, 4, 2, 2)
		case 'M003K:122':
		case '1xDial:122':
			return place(false, 2, 4, 2, 2)
		case 'M4315:111':
			return place(true, 0, -1, 4, 3)
		case 'M4315:143':
			return place(true, 2, -1, 4, 3)
		case 'M4315:22':
			return place(true, 0, 0, 4, 3)
		case 'M4315:132':
			return place(true, 2, 0, 4, 3)
		case 'M4315:23':
			return place(true, 0, 1, 4, 3)
		case 'M4315:112':
			return place(true, 1, 1, 4, 3)
		case 'M4315:134':
			return place(true, 2, 1, 4, 3)
		case 'M4315:113':
			return place(false, 0, -1, 4, 3)
		case 'M4315:24':
			return place(false, 0, 0, 4, 3)
		case 'M4315:42':
			return place(false, 0, 1, 4, 3)
		case 'M4315:41':
			return place(false, 0, 2, 4, 3)
		case 'M4315:122':
			return place(false, 0, 3, 4, 3)
		case 'M032LE:32':
			return place(true, 0, 1, 4, 1)
		case 'M032LE:21':
			return place(true, 0, 2, 4, 1)
		case 'M032LE:114':
			return place(true, 0, 3, 4, 1)
		case 'M032LE:141':
			return place(true, 2, 0, 4, 1)
		case 'M032LE:142':
			return place(true, 2, 1, 4, 1)
		case 'M032LE:131':
			return place(true, 2, 2, 4, 1)
		case 'M032LE:133':
			return place(true, 2, 3, 4, 1)
		case 'M032LE:134':
			return place(false, 0, 0, 4, 1)
		case 'M032LE:112':
			return place(false, 0, 1, 4, 1)
		case 'M032LE:23':
			return place(false, 0, 2, 4, 1)
		case 'M032LE:43':
			return place(false, 0, 3, 4, 1)
		case 'M032LE:44':
			return place(false, 0, 4, 4, 1)
		case 'M032LE:121':
			return place(false, 0, 5, 4, 1)
		case 'M005S:111':
			return place(true, 0, 0, 4, 2)
		case 'M005S:143':
			return place(true, 2, 0, 4, 2)
		case 'M005S:23':
			return place(true, 0, 2, 4, 2)
		case 'M005S:112':
			return place(true, 1, 2, 4, 2)
		case 'M005S:134':
			return place(true, 2, 2, 4, 2)
		case 'M005S:113':
			return place(false, 0, 0, 4, 2)
		case 'M005S:24':
			return place(false, 0, 1, 4, 2)
		case 'M005S:42':
			return place(false, 0, 2, 4, 2)
		case 'M005S:41':
			return place(false, 0, 3, 4, 2)
		case 'M005S:122':
			return place(false, 0, 4, 4, 2)
		default:
			return null
	}
}

export function gridKey(controls: readonly GridControl[]): string {
	return controls
		.map((control) => `${control.id}@${control.row},${control.column}`)
		.sort()
		.join('|')
}

export function buildGrid(modules: readonly string[], sites: Readonly<Record<string, string>>): SurfaceGrid | null {
	const visible = VISIBLE.filter((name) => modules.includes(name))
	if (visible.length === 0) {
		return null
	}
	const controls = layoutFromSites(visible, sites) ?? stackedLayout(visible)
	return {
		controls,
		hasKeypad: visible.includes('M4315'),
		hasSliders: visible.includes('M005S'),
		hasDial: visible.includes('1xDial'),
	}
}

function layoutFromSites(
	visible: readonly VisibleModule[],
	sites: Readonly<Record<string, string>>,
): GridControl[] | null {
	const controls: GridControl[] = []
	for (const name of visible) {
		const site = sites[name]
		if (!site) {
			return null
		}
		const place = placement(name, site)
		if (!place) {
			return null
		}
		controls.push(...controlsFor(name, place))
	}
	return withoutOverlap(controls)
}

function controlsFor(name: VisibleModule, place: Placement): GridControl[] {
	if (name === 'M4315') {
		return Array.from({ length: 15 }, (_, index) => ({
			id: `key/${index}`,
			row: place.y + Math.floor(index / 5),
			column: place.x + (index % 5),
			stylePreset: 'key' as const,
		}))
	}
	if (name === 'M003K') {
		return [
			{ id: 'knob/0', row: place.y, column: place.x, stylePreset: 'encoder' },
			{ id: 'knob/1', row: place.y + 1, column: place.x, stylePreset: 'encoder' },
			{ id: 'knob/2', row: place.y + 1, column: place.x + 1, stylePreset: 'encoder' },
		]
	}
	if (name === 'M032LE') {
		return [
			{ id: 'roller/1', row: place.y, column: place.x, stylePreset: 'encoder' },
			{ id: 'roller/0', row: place.y, column: place.x + place.w - 1, stylePreset: 'encoder' },
		]
	}
	if (name === 'M005S') {
		return Array.from({ length: 5 }, (_, index) => ({
			id: `slider/${index}`,
			row: place.y,
			column: place.x + index,
			stylePreset: 'encoder' as const,
		}))
	}
	return [{ id: 'dial', row: place.y, column: place.x, stylePreset: 'dial' }]
}

function withoutOverlap(controls: GridControl[]): GridControl[] | null {
	const first = controls[0]
	if (!first) {
		return null
	}
	const seen = new Set<string>()
	let minRow = first.row
	let minColumn = first.column
	for (const control of controls) {
		const key = `${control.row},${control.column}`
		if (seen.has(key)) {
			return null
		}
		seen.add(key)
		minRow = Math.min(minRow, control.row)
		minColumn = Math.min(minColumn, control.column)
	}
	return controls.map((control) => ({
		...control,
		row: control.row - minRow,
		column: control.column - minColumn,
	}))
}

function stackedLayout(visible: readonly VisibleModule[]): GridControl[] {
	const controls: GridControl[] = []
	let row = 0
	if (visible.includes('M4315')) {
		controls.push(
			...Array.from({ length: 15 }, (_, index) => ({
				id: `key/${index}`,
				row: row + Math.floor(index / 5),
				column: index % 5,
				stylePreset: 'key' as const,
			})),
		)
		row += 3
	}
	if (visible.includes('M032LE')) {
		controls.push(
			{ id: 'roller/1', row, column: 0, stylePreset: 'encoder' },
			{ id: 'roller/0', row, column: 1, stylePreset: 'encoder' },
		)
		row += 1
	}
	if (visible.includes('M005S')) {
		controls.push(
			...Array.from({ length: 5 }, (_, index) => ({
				id: `slider/${index}`,
				row,
				column: index,
				stylePreset: 'encoder' as const,
			})),
		)
		row += 1
	}
	const knobRow = row
	if (visible.includes('M003K')) {
		controls.push(
			{ id: 'knob/0', row: knobRow, column: 0, stylePreset: 'encoder' },
			{ id: 'knob/1', row: knobRow + 1, column: 0, stylePreset: 'encoder' },
			{ id: 'knob/2', row: knobRow + 1, column: 1, stylePreset: 'encoder' },
		)
	}
	if (visible.includes('1xDial')) {
		controls.push({
			id: 'dial',
			row: knobRow,
			column: visible.includes('M003K') ? 3 : 0,
			stylePreset: 'dial',
		})
	}
	return controls
}

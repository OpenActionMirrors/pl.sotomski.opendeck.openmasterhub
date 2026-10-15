import type ModuleInstance from './main.js'
import {
	LIGHTING_MODES,
	type CycleMode,
	type LightingMode,
	nextBrightness,
	nextCycle,
	nextToggle,
	rgbFromCompanionColor,
	selectedTargets,
} from './lighting.js'

export type ActionsSchema = {
	set_led_color: {
		options: {
			base: boolean
			top: boolean
			left: boolean
			right: boolean
			color: number
		}
	}
	set_led_mode: {
		options: {
			base: boolean
			top: boolean
			left: boolean
			right: boolean
			action: 'set' | 'toggle' | 'cycle'
			mode: LightingMode
		}
	}
	set_led_brightness: {
		options: {
			base: boolean
			top: boolean
			left: boolean
			right: boolean
			action: 'set' | 'increase' | 'decrease'
			value: number
			step: number
		}
	}
}

const targetOptions = [
	{ id: 'top' as const, label: 'Top knob' },
	{ id: 'left' as const, label: 'Left knob' },
	{ id: 'right' as const, label: 'Right knob' },
	{ id: 'base' as const, label: 'Base' },
]

function targetFields(): Array<{
	id: 'top' | 'left' | 'right' | 'base'
	type: 'checkbox'
	label: string
	default: boolean
}> {
	return targetOptions.map((target) => ({
		id: target.id,
		type: 'checkbox' as const,
		label: target.label,
		default: true,
	}))
}

export function UpdateActions(self: ModuleInstance): void {
	self.setActionDefinitions({
		set_led_color: {
			name: 'Set LED color',
			options: [
				...targetFields(),
				{
					id: 'color',
					type: 'colorpicker',
					label: 'Color',
					default: 0xff0000,
					returnType: 'number',
				},
			],
			callback: async (action) => {
				const { r, g, b } = rgbFromCompanionColor(action.options.color)
				await self.sendLed(selectedTargets(action.options), async (command) => {
					const client = self.client
					if (!client) {
						throw new Error('not connected')
					}
					await client.command('set_led_color', {
						module: command.module,
						control: command.control,
						r,
						g,
						b,
						save: false,
					})
				})
			},
		},
		set_led_mode: {
			name: 'Set LED mode',
			options: [
				...targetFields(),
				{
					id: 'action',
					type: 'dropdown',
					label: 'Action',
					default: 'set',
					choices: [
						{ id: 'set', label: 'Set' },
						{ id: 'toggle', label: 'On/off' },
						{ id: 'cycle', label: 'Next mode' },
					],
				},
				{
					id: 'mode',
					type: 'dropdown',
					label: 'Mode',
					default: 'static',
					choices: LIGHTING_MODES.map((mode) => ({ id: mode, label: modeLabel(mode) })),
					isVisibleExpression: `$(options:action) == "set"`,
				},
			],
			callback: async (action) => {
				let mode: LightingMode
				if (action.options.action === 'toggle') {
					mode = nextToggle(self.toggleMode)
					self.toggleMode = mode
				} else if (action.options.action === 'cycle') {
					mode = nextCycle(self.cycleMode)
					self.cycleMode = mode
				} else {
					mode = action.options.mode
				}
				await self.sendLed(selectedTargets(action.options), async (command) => {
					const client = self.client
					if (!client) {
						throw new Error('not connected')
					}
					await client.command('set_led_mode', {
						module: command.module,
						control: command.control,
						mode,
						save: false,
					})
				})
			},
		},
		set_led_brightness: {
			name: 'Set LED brightness',
			options: [
				...targetFields(),
				{
					id: 'action',
					type: 'dropdown',
					label: 'Action',
					default: 'set',
					choices: [
						{ id: 'set', label: 'Set' },
						{ id: 'increase', label: 'Increase' },
						{ id: 'decrease', label: 'Decrease' },
					],
				},
				{
					id: 'value',
					type: 'number',
					label: 'Brightness',
					default: 100,
					min: 0,
					max: 100,
					isVisibleExpression: `$(options:action) == "set"`,
				},
				{
					id: 'step',
					type: 'number',
					label: 'Step',
					default: 10,
					min: 1,
					max: 100,
					isVisibleExpression: `$(options:action) != "set"`,
				},
			],
			callback: async (action) => {
				const amount = action.options.action === 'set' ? action.options.value : action.options.step
				const brightness = nextBrightness(self.rgbBrightness, action.options.action, amount)
				self.rgbBrightness = brightness
				await self.sendLed(selectedTargets(action.options), async (command) => {
					const client = self.client
					if (!client) {
						throw new Error('not connected')
					}
					await client.command('set_led_brightness', {
						module: command.module,
						control: command.control,
						brightness,
					})
				})
			},
		},
	})
}

function modeLabel(mode: LightingMode | CycleMode): string {
	if (mode === 'disable') {
		return 'Off'
	}
	if (mode === 'static') {
		return 'Solid'
	}
	return mode.charAt(0).toUpperCase() + mode.slice(1)
}

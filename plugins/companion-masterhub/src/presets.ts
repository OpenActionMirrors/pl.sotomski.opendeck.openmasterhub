import type { CompanionPresetDefinitions, CompanionPresetSection } from '@companion-module/base'
import type { ModuleSchema } from './main.js'
import type ModuleInstance from './main.js'

const targets = { top: true, left: true, right: true, base: true }

export function UpdatePresets(self: ModuleInstance): void {
	const structure: CompanionPresetSection[] = [
		{
			id: 'lighting',
			name: 'Lighting',
			definitions: [
				{
					id: 'colors',
					name: 'Color',
					description: 'Set the knob and base LEDs',
					type: 'simple',
					presets: ['led_red'],
				},
				{
					id: 'modes',
					name: 'Mode',
					description: 'Set or clear an LED effect',
					type: 'simple',
					presets: ['led_rainbow', 'led_off'],
				},
				{
					id: 'brightness',
					name: 'Brightness',
					description: 'Step the RGB brightness',
					type: 'simple',
					presets: ['led_brighter', 'led_dimmer'],
				},
			],
		},
	]

	const presets: CompanionPresetDefinitions<ModuleSchema> = {
		led_red: {
			type: 'simple',
			name: 'Red',
			style: {
				text: 'Red',
				size: 'auto',
				color: 0xffffff,
				bgcolor: 0xff0000,
			},
			steps: [
				{
					down: [{ actionId: 'set_led_color', options: { ...targets, color: 0xff0000 } }],
					up: [],
				},
			],
			feedbacks: [],
		},
		led_rainbow: {
			type: 'simple',
			name: 'Rainbow',
			style: {
				text: 'Rainbow',
				size: 'auto',
				color: 0xffffff,
				bgcolor: 0x000000,
			},
			steps: [
				{
					down: [
						{
							actionId: 'set_led_mode',
							options: { ...targets, action: 'set', mode: 'rainbow' },
						},
					],
					up: [],
				},
			],
			feedbacks: [],
		},
		led_off: {
			type: 'simple',
			name: 'Lights off',
			style: {
				text: 'Off',
				size: 'auto',
				color: 0xffffff,
				bgcolor: 0x000000,
			},
			steps: [
				{
					down: [
						{
							actionId: 'set_led_mode',
							options: { ...targets, action: 'set', mode: 'disable' },
						},
					],
					up: [],
				},
			],
			feedbacks: [],
		},
		led_brighter: {
			type: 'simple',
			name: 'Brighter',
			style: {
				text: 'LED +',
				size: 'auto',
				color: 0xffffff,
				bgcolor: 0x000000,
			},
			steps: [
				{
					down: [
						{
							actionId: 'set_led_brightness',
							options: { ...targets, action: 'increase', value: 100, step: 10 },
						},
					],
					up: [],
				},
			],
			feedbacks: [],
		},
		led_dimmer: {
			type: 'simple',
			name: 'Dimmer',
			style: {
				text: 'LED -',
				size: 'auto',
				color: 0xffffff,
				bgcolor: 0x000000,
			},
			steps: [
				{
					down: [
						{
							actionId: 'set_led_brightness',
							options: { ...targets, action: 'decrease', value: 100, step: 10 },
						},
					],
					up: [],
				},
			],
			feedbacks: [],
		},
	}

	self.setPresetDefinitions(structure, presets)
}

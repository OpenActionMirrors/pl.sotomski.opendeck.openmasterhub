import type ModuleInstance from './main.js'
import { moduleIsConnected } from './lighting.js'

export type FeedbacksSchema = {
	module_connected: {
		type: 'boolean'
		options: {
			module: 'M4315' | 'M003K' | 'M032LE' | 'M005S' | 'base' | '1xDial'
		}
	}
}

export function UpdateFeedbacks(self: ModuleInstance): void {
	self.setFeedbackDefinitions({
		module_connected: {
			name: 'Module is connected',
			description: 'True when masterhubd lists that module. Base is also true when the dial is connected.',
			type: 'boolean',
			defaultStyle: {
				bgcolor: 0x00ff00,
				color: 0x000000,
			},
			options: [
				{
					id: 'module',
					type: 'dropdown',
					label: 'Module',
					default: 'M003K',
					choices: [
						{ id: 'M4315', label: 'Keypad (M4315)' },
						{ id: 'M003K', label: 'Knobs (M003K)' },
						{ id: 'M032LE', label: 'Rollers (M032LE)' },
						{ id: 'M005S', label: 'Sliders (M005S)' },
						{ id: 'base', label: 'Base (M0064 or dial)' },
						{ id: '1xDial', label: 'Dial (1xDial)' },
					],
				},
			],
			callback: (feedback) => moduleIsConnected(self.modules, feedback.options.module),
		},
	})
}

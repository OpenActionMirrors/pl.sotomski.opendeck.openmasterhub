import type ModuleInstance from './main.js'

export type VariablesSchema = {
	usb: string
	modules: string
}

export function UpdateVariableDefinitions(self: ModuleInstance): void {
	self.setVariableDefinitions({
		usb: { name: 'USB modules are open' },
		modules: { name: 'Open modules' },
	})
}

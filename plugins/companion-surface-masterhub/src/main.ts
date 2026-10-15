import type { SurfacePlugin } from '@companion-surface/base'
import { HubRemote, type PluginInfo } from './remote.js'

const remote = new HubRemote()

const plugin: SurfacePlugin<PluginInfo> = {
	remote,
	async init(): Promise<void> {},
	async destroy(): Promise<void> {
		await remote.destroy()
	},
	async openSurface(surfaceId, pluginInfo, context) {
		return remote.openSurface(surfaceId, pluginInfo, context)
	},
}

export default plugin

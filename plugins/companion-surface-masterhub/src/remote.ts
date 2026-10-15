import { EventEmitter } from 'node:events'
import type {
	OpenSurfaceResult,
	OptionsObject,
	SomeCompanionInputField,
	SurfaceContext,
	SurfacePluginRemote,
	SurfacePluginRemoteEvents,
} from '@companion-surface/base'
import { ensureLocalDaemon, MasterHubClient } from '@openmasterhub/sdk'
import { buildGrid, gridKey, type SurfaceGrid } from './layout.js'
import { MasterHubSurface, registerProps } from './surface.js'

export interface PluginInfo {
	connectionId: string
}

const RECONNECT_DELAYS_MS = [1000, 2000, 5000, 10000]
const SURFACE_ID = 'masterhub'

export class HubRemote
	extends EventEmitter<SurfacePluginRemoteEvents<PluginInfo>>
	implements SurfacePluginRemote<PluginInfo>
{
	readonly configFields: SomeCompanionInputField[] = [
		{
			id: 'host',
			type: 'textinput',
			label: 'Host',
			default: '127.0.0.1',
		},
		{
			id: 'port',
			type: 'number',
			label: 'Port',
			default: 9123,
			min: 1,
			max: 65535,
		},
	]
	readonly checkConfigMatchesExpression = '$(objA:host) == $(objB:host) && $(objA:port) == $(objB:port)'

	private readonly sessions = new Map<string, HubSession>()

	async startConnections(connections: Array<{ connectionId: string; config: OptionsObject }>): Promise<void> {
		for (const connection of connections) {
			await this.stopConnections([connection.connectionId])
			const session = new HubSession(connection.connectionId, endpoint(connection.config), (info) => {
				this.emit('surfacesConnected', [info])
			})
			this.sessions.set(connection.connectionId, session)
			session.start()
		}
	}

	async stopConnections(connectionIds: string[]): Promise<void> {
		for (const connectionId of connectionIds) {
			this.sessions.get(connectionId)?.stop()
			this.sessions.delete(connectionId)
		}
	}

	rejectSurface(surfaceInfo: { deviceHandle: string }): void {
		this.sessions.get(surfaceInfo.deviceHandle)?.reject()
	}

	async destroy(): Promise<void> {
		await this.stopConnections([...this.sessions.keys()])
	}

	openSurface(surfaceId: string, info: PluginInfo, context: SurfaceContext): OpenSurfaceResult {
		const session = this.sessions.get(info.connectionId)
		if (!session) {
			throw new Error('MasterHub connection is not running')
		}
		return session.open(surfaceId, context)
	}
}

class HubSession {
	private socket: MasterHubClient | null = null
	private surface: MasterHubSurface | null = null
	private grid: SurfaceGrid | null = null
	private layoutKey = ''
	private generation = 0
	private reconnectAttempt = 0
	private reconnectTimer: ReturnType<typeof setTimeout> | null = null
	private announceTimer: ReturnType<typeof setTimeout> | null = null
	private stopped = false
	private pendingOpen = false
	private rejected = false
	private warnedMissingDaemon = false

	constructor(
		private readonly connectionId: string,
		private readonly endpoint: string,
		private readonly announce: (info: {
			surfaceId: string
			deviceHandle: string
			description: string
			pluginInfo: PluginInfo
		}) => void,
	) {}

	start(): void {
		this.stopped = false
		this.connect()
	}

	stop(): void {
		this.stopped = true
		this.generation += 1
		this.clearTimers()
		this.socket?.close()
		this.socket = null
		this.surface?.disconnect(new Error('MasterHub connection stopped'))
		this.surface = null
	}

	reject(): void {
		this.pendingOpen = false
		this.rejected = true
	}

	open(surfaceId: string, context: SurfaceContext): OpenSurfaceResult {
		if (!this.grid) {
			throw new Error('MasterHub has no modules')
		}
		this.pendingOpen = false
		this.rejected = false
		const grid = this.grid
		const surface = new MasterHubSurface(surfaceId, grid, context, this, () => {
			if (this.surface === surface) {
				this.surface = null
			}
		})
		this.surface = surface
		return { surface, registerProps: registerProps(grid) }
	}

	get client(): MasterHubClient | null {
		return this.socket
	}

	private connect(): void {
		if (this.stopped) {
			return
		}
		this.clearReconnect()
		this.generation += 1
		const generation = this.generation
		this.socket?.close()
		const client = new MasterHubClient({ endpoint: this.endpoint })
		this.socket = client
		client.onEvent((event) => {
			if (generation !== this.generation) {
				return
			}
			if (event.action === 'modules') {
				this.apply(event.payload.modules, event.payload.sites)
				return
			}
			this.surface?.handle(event)
		})
		client.onDisconnect(() => {
			if (generation !== this.generation || this.stopped) {
				return
			}
			this.drop()
		})
		void client.connect().then(
			(hello) => {
				if (generation !== this.generation || this.stopped) {
					return
				}
				this.reconnectAttempt = 0
				this.apply(hello.modules, hello.sites)
			},
			() => {
				if (generation !== this.generation || this.stopped) {
					return
				}
				this.drop()
			},
		)
	}

	private apply(modules: string[], sites: Record<string, string>): void {
		const grid = buildGrid(modules, sites)
		const key = grid ? gridKey(grid.controls) : ''
		if (!grid) {
			this.grid = null
			this.layoutKey = ''
			this.pendingOpen = false
			this.surface?.disconnect(new Error('MasterHub has no modules'))
			this.surface = null
			return
		}
		if (key === this.layoutKey && (this.surface || this.pendingOpen || this.rejected)) {
			this.grid = grid
			return
		}
		this.grid = grid
		this.layoutKey = key
		this.rejected = false
		if (this.surface) {
			this.surface.disconnect(new Error('MasterHub module layout changed'))
			this.surface = null
		}
		this.pendingOpen = true
		this.scheduleAnnounce()
	}

	private drop(): void {
		this.layoutKey = ''
		this.pendingOpen = false
		this.rejected = false
		this.surface?.disconnect(new Error('masterhubd connection closed'))
		this.surface = null
		void this.recover(this.generation)
	}

	private async recover(generation: number): Promise<void> {
		try {
			const result = await ensureLocalDaemon(this.endpoint, import.meta.dirname)
			if (generation !== this.generation || this.stopped) {
				return
			}
			if (result.state === 'started') {
				this.warnedMissingDaemon = false
				console.info('Started masterhubd')
			} else if (result.state === 'missing' && !this.warnedMissingDaemon) {
				this.warnedMissingDaemon = true
				console.warn('masterhubd is not running and no binary was found')
			}
		} catch (error) {
			if (generation !== this.generation || this.stopped) {
				return
			}
			console.warn(error instanceof Error ? error.message : 'failed to start masterhubd')
		}
		if (generation !== this.generation || this.stopped) {
			return
		}
		this.scheduleReconnect()
	}

	private scheduleAnnounce(): void {
		if (this.announceTimer) {
			clearTimeout(this.announceTimer)
		}
		this.announceTimer = setTimeout(() => {
			this.announceTimer = null
			if (this.stopped || !this.grid || !this.pendingOpen) {
				return
			}
			this.announce({
				surfaceId: SURFACE_ID,
				deviceHandle: this.connectionId,
				description: 'MasterHub',
				pluginInfo: { connectionId: this.connectionId },
			})
		}, 0)
	}

	private scheduleReconnect(): void {
		this.clearReconnect()
		const delay = RECONNECT_DELAYS_MS[Math.min(this.reconnectAttempt, RECONNECT_DELAYS_MS.length - 1)] ?? 10000
		this.reconnectAttempt += 1
		this.reconnectTimer = setTimeout(() => {
			this.reconnectTimer = null
			this.connect()
		}, delay)
	}

	private clearReconnect(): void {
		if (this.reconnectTimer) {
			clearTimeout(this.reconnectTimer)
			this.reconnectTimer = null
		}
	}

	private clearTimers(): void {
		this.clearReconnect()
		if (this.announceTimer) {
			clearTimeout(this.announceTimer)
			this.announceTimer = null
		}
	}
}

function endpoint(config: OptionsObject): string {
	const host = typeof config.host === 'string' && config.host.trim() ? config.host.trim() : '127.0.0.1'
	const port = typeof config.port === 'number' ? config.port : Number(config.port)
	const safePort = Number.isInteger(port) && port >= 1 && port <= 65535 ? port : 9123
	return `ws://${host}:${safePort}`
}

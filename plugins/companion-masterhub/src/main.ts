import { InstanceBase, InstanceStatus, type SomeCompanionConfigField } from '@companion-module/base'
import { ensureLocalDaemon, MasterHubClient, MasterHubError, type EventMessage } from '@openmasterhub/sdk'
import { GetConfigFields, type ModuleConfig } from './config.js'
import { UpdateActions, type ActionsSchema } from './actions.js'
import { UpdateFeedbacks, type FeedbacksSchema } from './feedbacks.js'
import { ledCommands, type CycleMode, type LedCommand } from './lighting.js'
import { UpdatePresets } from './presets.js'
import { UpgradeScripts } from './upgrades.js'
import { UpdateVariableDefinitions, type VariablesSchema } from './variables.js'

export type ModuleSchema = {
	config: ModuleConfig
	secrets: undefined
	actions: ActionsSchema
	feedbacks: FeedbacksSchema
	variables: VariablesSchema
}

const RECONNECT_DELAYS_MS = [1000, 2000, 5000, 10000]

export { UpgradeScripts }

export default class ModuleInstance extends InstanceBase<ModuleSchema> {
	config!: ModuleConfig
	client: MasterHubClient | null = null
	modules: string[] = []
	toggleMode: 'disable' | 'static' | undefined
	cycleMode: CycleMode | undefined
	rgbBrightness = 100

	private generation = 0
	private reconnectAttempt = 0
	private reconnectTimer: ReturnType<typeof setTimeout> | null = null
	private stopped = false
	private warnedMissingDaemon = false

	constructor(internal: unknown) {
		super(internal)
	}

	async init(config: ModuleConfig): Promise<void> {
		this.config = config
		this.updateActions()
		this.updateFeedbacks()
		this.updatePresets()
		this.updateVariableDefinitions()
		this.publishModules(false, [])
		this.connect()
	}

	async destroy(): Promise<void> {
		this.stopped = true
		this.generation += 1
		this.clearReconnect()
		this.client?.close()
		this.client = null
	}

	async configUpdated(config: ModuleConfig): Promise<void> {
		const changed = config.host !== this.config.host || config.port !== this.config.port
		this.config = config
		if (changed) {
			this.reconnectAttempt = 0
			this.connect()
		}
	}

	getConfigFields(): SomeCompanionConfigField[] {
		return GetConfigFields()
	}

	updateActions(): void {
		UpdateActions(this)
	}

	updateFeedbacks(): void {
		UpdateFeedbacks(this)
	}

	updatePresets(): void {
		UpdatePresets(this)
	}

	updateVariableDefinitions(): void {
		UpdateVariableDefinitions(this)
	}

	async sendLed(
		targets: Parameters<typeof ledCommands>[0],
		send: (command: LedCommand) => Promise<unknown>,
	): Promise<void> {
		const commands = ledCommands(targets)
		if (commands.length === 0) {
			this.log('warn', 'No LED targets selected')
			return
		}
		try {
			await Promise.all(
				commands.map(async (command) => {
					await send(command)
				}),
			)
		} catch (error) {
			const message = error instanceof Error ? error.message : 'LED command failed'
			this.log('warn', message)
		}
	}

	private connect(): void {
		if (this.stopped) {
			return
		}
		this.clearReconnect()
		const endpoint = this.endpoint()
		if (!endpoint) {
			this.updateStatus(InstanceStatus.BadConfig, 'Host and port are required')
			return
		}
		this.generation += 1
		const generation = this.generation
		this.client?.close()
		const client = new MasterHubClient({ endpoint })
		this.client = client
		this.updateStatus(InstanceStatus.Connecting)
		client.onEvent((event) => {
			if (generation !== this.generation) {
				return
			}
			this.onDaemonEvent(event)
		})
		client.onDisconnect((error) => {
			if (generation !== this.generation || this.stopped) {
				return
			}
			this.updateStatus(InstanceStatus.ConnectionFailure, error.message)
			void this.recover(generation)
		})
		void client.connect().then(
			(hello) => {
				if (generation !== this.generation || this.stopped) {
					return
				}
				this.reconnectAttempt = 0
				this.publishModules(hello.usb, hello.modules)
				this.updateStatus(InstanceStatus.Ok)
			},
			(error: unknown) => {
				if (generation !== this.generation || this.stopped) {
					return
				}
				const message = error instanceof MasterHubError ? error.message : 'connection failed'
				this.updateStatus(InstanceStatus.ConnectionFailure, message)
				void this.recover(generation)
			},
		)
	}

	private onDaemonEvent(event: EventMessage): void {
		if (event.action !== 'modules') {
			return
		}
		this.publishModules(event.payload.modules.length > 0, event.payload.modules)
	}

	private publishModules(usb: boolean, modules: string[]): void {
		this.modules = [...modules]
		this.setVariableValues({
			usb: usb ? 'true' : 'false',
			modules: modules.join(', '),
		})
		this.checkFeedbacks('module_connected')
	}

	private endpoint(): string | null {
		const host = this.config.host?.trim()
		const port = this.config.port
		if (!host || !Number.isInteger(port) || port < 1 || port > 65535) {
			return null
		}
		return `ws://${host}:${port}`
	}

	private async recover(generation: number): Promise<void> {
		const endpoint = this.endpoint()
		if (endpoint && generation === this.generation && !this.stopped) {
			try {
				const result = await ensureLocalDaemon(endpoint, import.meta.dirname)
				if (generation !== this.generation || this.stopped) {
					return
				}
				if (result.state === 'started') {
					this.warnedMissingDaemon = false
					this.log('info', 'Started masterhubd')
				} else if (result.state === 'missing' && !this.warnedMissingDaemon) {
					this.warnedMissingDaemon = true
					this.log('warn', 'masterhubd is not running and no binary was found')
				}
			} catch (error) {
				if (generation !== this.generation || this.stopped) {
					return
				}
				const message = error instanceof Error ? error.message : 'failed to start masterhubd'
				this.log('warn', message)
			}
		}
		if (generation !== this.generation || this.stopped) {
			return
		}
		this.scheduleReconnect()
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
}

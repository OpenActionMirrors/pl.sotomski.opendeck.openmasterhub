import assert from 'node:assert/strict'
import { test } from 'node:test'

import { mapDaemonEvent, SliderMemory } from './input.js'

const controls = new Set(['key/4', 'knob/1', 'roller/0', 'dial', 'slider/2'])

function event(action: string, payload: unknown) {
	return { action, payload }
}

void test('keys, knobs, the dial, and rollers become presses and detents', () => {
	const sliders = new SliderMemory()
	assert.deepEqual(
		mapDaemonEvent(
			event('key_state', { module: { model: 'M4315', id: 'keys' }, control: 4, pressed: true }),
			sliders,
			controls,
		),
		{ controlId: 'key/4', pressed: true },
	)
	assert.deepEqual(
		mapDaemonEvent(
			event('knob_rotated', { module: { model: 'M003K', id: 'knobs' }, control: 1, direction: 'ccw' }),
			sliders,
			controls,
		),
		{ controlId: 'knob/1', delta: -1 },
	)
	assert.deepEqual(
		mapDaemonEvent(
			event('knob_rotated', { module: { model: 'M0064', id: 'base' }, control: 0, direction: 'cw' }),
			sliders,
			controls,
		),
		{ controlId: 'dial', delta: 1 },
	)
	assert.deepEqual(
		mapDaemonEvent(
			event('roller_rotated', { module: { model: 'M032LE', id: 'rollers' }, control: 0, direction: 'up' }),
			sliders,
			controls,
		),
		{ controlId: 'roller/0', delta: 1 },
	)
	assert.equal(
		mapDaemonEvent(
			event('key_state', { module: { model: 'M4315', id: 'keys' }, control: 1, pressed: true }),
			sliders,
			controls,
		),
		null,
	)
})

void test('the first slider sample does not rotate', () => {
	const sliders = new SliderMemory()
	const payload = { module: { model: 'M005S', id: 'faders' }, control: 2 }
	assert.deepEqual(mapDaemonEvent(event('slider_moved', { ...payload, value: 40 }), sliders, controls), {
		controlId: 'slider/2',
		slider: 40,
	})
	assert.deepEqual(mapDaemonEvent(event('slider_moved', { ...payload, value: 40 }), sliders, controls), {
		controlId: 'slider/2',
		slider: 40,
	})
	assert.deepEqual(mapDaemonEvent(event('slider_moved', { ...payload, value: 35 }), sliders, controls), {
		controlId: 'slider/2',
		slider: 35,
		delta: -5,
	})
})

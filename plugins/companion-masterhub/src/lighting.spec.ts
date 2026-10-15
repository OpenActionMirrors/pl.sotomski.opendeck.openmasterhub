import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
	ledCommands,
	moduleIsConnected,
	nextBrightness,
	nextCycle,
	nextToggle,
	rgbFromCompanionColor,
} from './lighting.js'

void test('all three knobs become one command and the base stays separate', () => {
	assert.deepEqual(ledCommands(['top', 'left', 'right', 'base']), [
		{ module: { model: 'M003K', id: 'knobs' }, control: 'all' },
		{ module: { model: 'M0064', id: 'base' }, control: 0 },
	])
})

void test('a subset of knobs keeps its indexes', () => {
	assert.deepEqual(ledCommands(['right', 'top']), [
		{ module: { model: 'M003K', id: 'knobs' }, control: 0 },
		{ module: { model: 'M003K', id: 'knobs' }, control: 2 },
	])
	assert.deepEqual(ledCommands(['base']), [{ module: { model: 'M0064', id: 'base' }, control: 0 }])
	assert.deepEqual(ledCommands([]), [])
})

void test('toggle starts at solid and cycle skips off', () => {
	assert.equal(nextToggle(undefined), 'static')
	assert.equal(nextToggle('static'), 'disable')
	assert.equal(nextToggle('disable'), 'static')
	assert.equal(nextCycle(undefined), 'static')
	assert.equal(nextCycle('static'), 'breathing')
	assert.equal(nextCycle('rainbow'), 'static')
	assert.equal(nextCycle('disable'), 'static')
})

void test('brightness stays inside 0 to 100', () => {
	assert.equal(nextBrightness(100, 'set', 40), 40)
	assert.equal(nextBrightness(100, 'increase', 10), 100)
	assert.equal(nextBrightness(3, 'decrease', 10), 0)
	assert.deepEqual(rgbFromCompanionColor(0xff0000), { r: 255, g: 0, b: 0 })
})

void test('the base feedback is true for the base or the dial', () => {
	assert.equal(moduleIsConnected(['1xDial'], 'base'), true)
	assert.equal(moduleIsConnected(['M0064'], 'base'), true)
	assert.equal(moduleIsConnected(['M003K'], 'base'), false)
	assert.equal(moduleIsConnected(['M003K'], 'M003K'), true)
})

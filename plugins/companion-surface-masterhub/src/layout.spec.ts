import assert from 'node:assert/strict'
import { test } from 'node:test'

import { buildGrid } from './layout.js'

void test('the base alone is not a surface', () => {
	assert.equal(buildGrid(['M0064'], {}), null)
	assert.equal(buildGrid([], { M4315: '22' }), null)
})

void test('a keypad above the base shifts so the first row is zero', () => {
	const grid = buildGrid(['M4315'], { M4315: '111' })
	assert.ok(grid)
	assert.deepEqual(
		grid.controls.find((control) => control.id === 'key/0'),
		{ id: 'key/0', row: 0, column: 0, stylePreset: 'key' },
	)
	assert.deepEqual(
		grid.controls.find((control) => control.id === 'key/5'),
		{ id: 'key/5', row: 1, column: 0, stylePreset: 'key' },
	)
})

void test('knobs and the dial keep their sites when the cells do not overlap', () => {
	const grid = buildGrid(['M003K', '1xDial'], { M003K: '111', '1xDial': '124' })
	assert.ok(grid)
	assert.deepEqual(
		grid.controls.find((control) => control.id === 'dial'),
		{
			id: 'dial',
			row: 0,
			column: 0,
			stylePreset: 'dial',
		},
	)
	assert.deepEqual(
		grid.controls.find((control) => control.id === 'knob/0'),
		{
			id: 'knob/0',
			row: 0,
			column: 2,
			stylePreset: 'encoder',
		},
	)
	assert.equal(grid.controls.find((control) => control.id === 'knob/2')?.column, 3)
})

void test('overlapping sites fall back to the stacked grid', () => {
	const grid = buildGrid(['M4315', 'M003K'], { M4315: '22', M003K: '124' })
	assert.ok(grid)
	assert.equal(grid.controls.find((control) => control.id === 'key/0')?.row, 0)
	assert.equal(grid.controls.find((control) => control.id === 'knob/0')?.row, 3)
})

void test('missing sites stack keys, rollers, sliders, then knobs beside the dial', () => {
	const grid = buildGrid(['1xDial', 'M005S', 'M032LE', 'M003K', 'M4315'], {})
	assert.ok(grid)
	assert.equal(grid.controls.find((control) => control.id === 'key/14')?.row, 2)
	assert.deepEqual(
		grid.controls.find((control) => control.id === 'roller/1'),
		{
			id: 'roller/1',
			row: 3,
			column: 0,
			stylePreset: 'encoder',
		},
	)
	assert.equal(grid.controls.find((control) => control.id === 'roller/0')?.column, 1)
	assert.equal(grid.controls.find((control) => control.id === 'slider/4')?.column, 4)
	assert.equal(grid.controls.find((control) => control.id === 'slider/0')?.row, 4)
	assert.equal(grid.controls.find((control) => control.id === 'knob/0')?.row, 5)
	assert.equal(grid.controls.find((control) => control.id === 'dial')?.column, 3)
})

void test('hardware roller 0 sits on the right of its site', () => {
	const grid = buildGrid(['M032LE'], { M032LE: '32' })
	assert.ok(grid)
	assert.equal(grid.controls.find((control) => control.id === 'roller/1')?.column, 0)
	assert.equal(grid.controls.find((control) => control.id === 'roller/0')?.column, 3)
	assert.equal(grid.controls.find((control) => control.id === 'roller/0')?.row, 0)
})

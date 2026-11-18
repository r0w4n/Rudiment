import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHighwayController } from '../../web/js/highway-controller.mjs';

const exercise = () => ({
  meter_numerator: 4, meter_denominator: 4, ticks_per_beat: 480,
  hits: [0, 0.5, 4, 8, 12].map((beat, index) => ({
    id: `note-${index}`, beat, instrument: 'snare', raw_duration_ticks: 120,
    hand: index % 2 ? 'L' : 'R', accent: index === 0,
  })),
});

function setup() {
  const calls = [];
  const renderer = {
    initialize: () => calls.push(['initialize']),
    update: model => calls.push(['update', model]),
    resize: viewport => calls.push(['resize', viewport]),
    dispose: () => calls.push(['dispose']),
  };
  const controller = createHighwayController(renderer);
  const latest = () => calls.filter(call => call[0] === 'update').at(-1)?.[1];
  return { calls, controller, latest };
}

test('presents musical positions, durations, grid and detached feedback', () => {
  const { controller, latest } = setup();
  const source = exercise();
  controller.setExercise(source);
  controller.setPosition(0.5);
  controller.setJudgements([
    { expected_id: 'note-1', judgement: 'good' },
    { expected_id: null, event_id: 'extra-1', actual_beat: 1.25, iteration: 0, instrument: 'snare', judgement: 'extra' },
  ]);
  const model = latest();
  assert.equal(model.position, 0.5);
  assert.deepEqual(model.visibleRange, { start: -1.5, end: 6.5 });
  assert.equal(model.notes.find(note => note.id === 'note-1').position, 0.5);
  assert.equal(model.notes.find(note => note.id === 'note-1').duration, 0.25);
  assert.equal(model.notes.find(note => note.id === 'note-1').judgement, 'good');
  assert.deepEqual(model.unmatched, [{ id: 'extra-1', instrument: 'snare', judgement: 'extra', iteration: 0, position: 1.25 }]);
  assert.ok(model.boundaries.some(boundary => boundary.position === 4 && boundary.kind === 'bar' && boundary.bar === 2));
  assert.ok(model.boundaries.some(boundary => boundary.position === 0.25 && boundary.kind === 'subdivision'));
  assert.throws(() => { model.notes[0].beat = 999; }, TypeError);
  assert.equal(source.hits[0].beat, 0);
});

test('maps selected noncontiguous bars to consecutive playback positions and stops at end', () => {
  const { controller, latest } = setup();
  controller.setExercise(exercise());
  controller.setTimeline([0, 2], 16);
  controller.setPosition(3.5);
  const model = latest();
  const next = model.notes.find(note => note.id === 'note-3');
  assert.equal(next.beat, 8);
  assert.equal(next.position, 4);
  assert.ok(model.notes.some(note => note.id === 'note-0' && note.position === 8));
  controller.setJudgements([{ expected_id: null, event_id: 'selected-extra', actual_beat: 8.25, iteration: 0, instrument: 'snare', judgement: 'extra' }]);
  assert.equal(latest().unmatched[0].position, 4.25);
  controller.setPosition(16);
  assert.ok(latest().notes.every(note => note.position < 16));
});

test('rejects unsupported instruments without replacing the active model', () => {
  const { controller, latest } = setup();
  controller.setExercise(exercise());
  const original = latest();
  const unsupported = exercise();
  unsupported.hits.push({ id: 'kick', beat: 1, instrument: 'kick' });
  assert.throws(() => controller.setExercise(unsupported), /kick/);
  controller.setPosition(1);
  assert.equal(latest().notes[0].id, original.notes[0].id);
});

test('feedback is bound to the matched repetition, including an early next-loop hit', () => {
  const { controller, latest } = setup();
  controller.setExercise(exercise());
  controller.setTimeline([0], 12);
  controller.setPosition(3.9);
  controller.setJudgements([{ expected_id: 'note-0', judgement: 'early', iteration: 1 }]);
  const instances = latest().notes.filter(note => note.id === 'note-0');
  assert.equal(instances.find(note => note.iteration === 1).judgement, 'early');
  assert.equal(instances.find(note => note.iteration === 2).judgement, null);
});

test('bar membership follows scoring notation onset without quantizing MIDI timing', () => {
  const { controller, latest } = setup();
  const source = exercise();
  source.hits.push({ id: 'near-bar', beat: 7.99, notation_beat: 8, instrument: 'snare', raw_duration_ticks: null, hand: 'R' });
  controller.setExercise(source);
  controller.setTimeline([2], 12);
  controller.setPosition(8);
  const note = latest().notes.find(item => item.id === 'near-bar');
  assert.equal(note.beat, 7.99);
  assert.equal(note.position, 7.99);
});

test('a renderer that fails during initialization is disposed', () => {
  let disposed = 0;
  assert.throws(() => createHighwayController({
    initialize() { throw new Error('initialize failed'); },
    update() {}, resize() {}, dispose() { disposed++; },
  }), /initialize failed/);
  assert.equal(disposed, 1);
});

test('forwards viewport changes and disposes exactly once', () => {
  const { controller, calls } = setup();
  const viewport = { width: 800, height: 300 };
  controller.resize(viewport);
  viewport.width = 1;
  assert.equal(calls.find(call => call[0] === 'resize')[1].width, 800);
  controller.dispose();
  controller.dispose();
  assert.equal(calls.filter(call => call[0] === 'dispose').length, 1);
  assert.throws(() => controller.setPosition(1), /disposed/);
});

test('grid beats follow the meter denominator', () => {
  const { controller, latest } = setup();
  const source = exercise();
  source.meter_numerator = 6;
  source.meter_denominator = 8;
  controller.setExercise(source);
  assert.ok(latest().boundaries.some(boundary => boundary.position === 0.5 && boundary.kind === 'beat'));
  assert.ok(latest().boundaries.some(boundary => boundary.position === 3 && boundary.kind === 'bar'));
});

test('preserves the domain grace-note flag for renderers', () => {
  const { controller, latest } = setup();
  const source = exercise();
  source.hits[1].grace = true;
  controller.setExercise(source);
  assert.equal(latest().notes.find(note => note.id === 'note-1').grace, true);
  assert.equal(latest().notes.find(note => note.id === 'note-0').grace, false);
});

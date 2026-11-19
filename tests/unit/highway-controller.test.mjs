import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHighwayController } from '../../web/js/highway-controller.mjs';
import { createPresentationHost } from '../../web/js/presentation-host.mjs';

const exercise = () => ({
  meter_numerator: 4, meter_denominator: 4, ticks_per_beat: 480,
  hits: [0, 0.5, 4, 8, 12].map((beat, index) => ({
    id: `note-${index}`, beat, source_bar: Math.floor(beat / 4), instrument: 'snare', raw_duration_ticks: 120,
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
  assert.deepEqual(model.unmatched, [{ id: 'extra-1', instrument: 'snare', lane: 'snare', judgement: 'extra', iteration: 0, position: 1.25 }]);
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

test('reuses static projections between beats but refreshes on boundaries, feedback and seeks', () => {
  const { controller, latest } = setup();
  controller.setExercise(exercise());
  controller.setTimeline([0, 2], 16);
  controller.setPosition(0.1);
  const first = latest();
  controller.setPosition(0.9);
  assert.equal(latest().position, 0.9);
  assert.equal(latest().notes, first.notes);
  assert.equal(latest().boundaries, first.boundaries);
  assert.equal(latest().timeline, first.timeline);
  assert.equal(latest().playback, first.playback);
  controller.setJudgements([{ expected_id: 'note-0', judgement: 'early', iteration: 0 }]);
  assert.notEqual(latest().notes, first.notes);
  assert.equal(latest().notes.find(note => note.id === 'note-0').judgement, 'early');
  const withFeedback = latest().notes;
  controller.setPosition(1.1);
  assert.notEqual(latest().notes, withFeedback);
  controller.setPosition(8.2);
  assert.equal(latest().timeline.iteration, 1);
  assert.equal(latest().notes.find(note => note.id === 'note-0' && note.iteration === 1).position, 8);
  controller.dispose();
});

test('maps kick, snare and both hi-hat articulations onto configured lanes', () => {
  const { controller, latest } = setup();
  const source = exercise();
  source.hits.push(
    { id: 'kick', beat: 1, source_bar: 0, instrument: 'kick', articulation: 'center' },
    { id: 'closed', beat: 1, source_bar: 0, instrument: 'closed_hat', articulation: 'closed' },
    { id: 'open', beat: 1.5, source_bar: 0, instrument: 'open_hat', articulation: 'open' },
  );
  controller.setExercise(source);
  assert.equal(latest().notes.find(note => note.id === 'kick').lane, 'kick');
  assert.equal(latest().notes.find(note => note.id === 'closed').lane, 'hi_hat');
  assert.equal(latest().notes.find(note => note.id === 'open').lane, 'hi_hat');
  assert.deepEqual(latest().lanes.map(lane => lane.label), ['KICK', 'SNARE', 'HI-HAT']);
});

test('shows unsupported live strikes in a separate lane without accepting unsupported exercises', () => {
  const { controller, latest } = setup();
  controller.setExercise(exercise());
  controller.setJudgements([{ expected_id: null, event_id: 'ride-hit', actual_beat: 0.5, iteration: 0, instrument: 'ride', judgement: 'wrong_instrument' }]);
  assert.equal(latest().unmatched[0].lane, 'other');
  assert.deepEqual(latest().lanes.map(lane => lane.instrument), ['kick', 'snare', 'hi_hat', 'other']);
  controller.setJudgements([]);
  assert.deepEqual(latest().lanes.map(lane => lane.instrument), ['kick', 'snare', 'hi_hat']);
  assert.throws(() => controller.setExercise({ ...exercise(), hits: [{ id: 'ride', beat: 0, instrument: 'ride' }] }), /ride/);
});

test('rejects unsupported instruments without replacing the active model', () => {
  const { controller, latest } = setup();
  controller.setExercise(exercise());
  const original = latest();
  const unsupported = exercise();
  unsupported.hits.push({ id: 'ride', beat: 1, instrument: 'ride' });
  assert.throws(() => controller.setExercise(unsupported), /ride/);
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
  source.hits.push({ id: 'near-bar', beat: 7.99, notation_beat: 8, source_bar: 2, instrument: 'snare', raw_duration_ticks: null, hand: 'R' });
  controller.setExercise(source);
  controller.setTimeline([2], 12);
  controller.setPosition(8);
  const note = latest().notes.find(item => item.id === 'near-bar');
  assert.equal(note.beat, 7.99);
  assert.equal(note.position, 7.99);
});

test('requires Rust-assigned source bars instead of reclassifying written onsets', () => {
  const { controller, latest } = setup();
  const source = exercise();
  source.hits.push({ id: 'assigned', beat: 7.99, source_bar: 2, instrument: 'snare' });
  controller.setExercise(source);
  controller.setTimeline([2], 12);
  controller.setPosition(8);
  assert.equal(latest().notes.find(note => note.id === 'assigned').position, 7.99);
  assert.throws(() => controller.setExercise({ ...source, hits: [{ id: 'missing-bar', beat: 0, instrument: 'snare' }] }), /Invalid expected note/);
});

test('a faulty optional renderer detaches without keeping presentation active', () => {
  const failures = [];
  let disposed = 0;
  let fail = false;
  const host = createPresentationHost(() => null, error => failures.push(error));
  host.bind(() => ({
    exercise: exercise(),
    config: { bars: [0], endBeat: 4, position: 0, phase: 'idle', tempoBpm: 60, loops: 1, repeatUntilGood: false, barAttempt: 0 },
    judgements: [],
  }));
  host.mount({
    initialize() {},
    update() { if (fail) throw new Error('renderer failed'); },
    resize() {},
    dispose() { disposed++; },
  });
  fail = true;
  host.position(0, 60);
  assert.equal(host.isActive(), false);
  assert.equal(disposed, 1);
  assert.equal(failures.length, 1);
  host.unbind();
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

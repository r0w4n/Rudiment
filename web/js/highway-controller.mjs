// @ts-check
// Renderer-independent highway presentation. The application supplies beats and
// judgements; this module neither reads a clock nor changes musical state.
/** @typedef {import('./presentation-types.mjs').Exercise} Exercise */
/** @typedef {import('./presentation-types.mjs').Lane} Lane */
/** @typedef {import('./presentation-types.mjs').Timeline} Timeline */
/** @typedef {import('./presentation-types.mjs').SourceNote} SourceNote */
/** @typedef {import('./presentation-types.mjs').Playback} Playback */
/** @typedef {import('./presentation-types.mjs').Renderer} Renderer */
/** @typedef {import('./presentation-types.mjs').Judgement} Judgement */
const EPSILON = 1e-9;
/** @type {readonly Lane[]} */
export const DEFAULT_HIGHWAY_LANES = Object.freeze([
  Object.freeze({ instrument: 'kick', label: 'KICK', instruments: Object.freeze(['kick']) }),
  Object.freeze({ instrument: 'snare', label: 'SNARE', instruments: Object.freeze(['snare']) }),
  Object.freeze({ instrument: 'hi_hat', label: 'HI-HAT', instruments: Object.freeze(['closed_hat', 'open_hat', 'pedal_hat']) }),
]);
const OTHER_LANE = Object.freeze({ instrument: 'other', label: 'OTHER', instruments: Object.freeze([]) });

/** @param {Timeline['notes']} notes
 * @param {number} beat
 */
function lowerBound(notes, beat) {
  let low = 0;
  let high = notes.length;
  while (low < high) {
    const middle = (low + high) >>> 1;
    if (notes[middle].sequenceBeat < beat) low = middle + 1;
    else high = middle;
  }
  return low;
}

/**
 * A renderer receives detached, immutable presentation snapshots. It must not
 * be given the transport or any scoring callbacks.
 * @param {Renderer} renderer
 * @param {readonly Lane[]} lanes
 */
export function createHighwayController(renderer, lanes = DEFAULT_HIGHWAY_LANES) {
  const configuredLanes = lanes.map(lane => Object.freeze({
    ...lane,
    instruments: Object.freeze([...(lane.instruments ?? [lane.instrument])]),
  }));
  const instrumentLanes = new Map();
  for (const lane of configuredLanes) for (const instrument of lane.instruments) instrumentLanes.set(instrument, lane.instrument);
  /** @type {{barBeats: number; beatUnit: number; barCount: number; notes: SourceNote[]} | null} */
  let exercise = null;
  /** @type {Timeline | null} */
  let sequence = null;
  let position = 0;
  let before = 2;
  let after = 6;
  /** @type {Map<number, Map<string, string>>} */
  let judgements = new Map();
  /** @type {import('./presentation-types.mjs').Presentation['unmatched'][number][]} */
  let unmatched = [];
  /** @type {Playback} */
  let playback = Object.freeze({ phase: 'idle', tempoBpm: null });
  let disposed = false;

  function assertActive() {
    if (disposed) throw new Error('Highway controller has been disposed');
  }

  function update() {
    if (!exercise || !sequence) return;
    const start = position - before;
    const end = position + after;
    /** @type {import('./presentation-types.mjs').Presentation['notes'][number][]} */
    const notes = [];
    /** @type {import('./presentation-types.mjs').Presentation['boundaries'][number][]} */
    const boundaries = [];
    const visibleUnmatched = unmatched.filter(hit => hit.position >= start - EPSILON && hit.position <= end + EPSILON).map(hit => Object.freeze({ ...hit }));
    const visibleLanes = visibleUnmatched.some(hit => hit.lane === 'other' && !configuredLanes.some(lane => lane.instrument === 'other'))
      ? [...configuredLanes, OTHER_LANE] : configuredLanes;
    const length = sequence.bars.length * exercise.barBeats;
    const firstCycle = Math.max(0, Math.floor((start - sequence.origin) / length));
    const lastCycle = Math.max(-1, Math.floor((end - sequence.origin) / length));
    for (let cycle = firstCycle; cycle <= lastCycle; cycle++) {
      const base = sequence.origin + cycle * length;
      if (base >= sequence.endBeat - EPSILON) break;
      for (let index = lowerBound(sequence.notes, start - base); index < sequence.notes.length; index++) {
        const note = sequence.notes[index];
        const beat = base + note.sequenceBeat;
        if (beat > end + EPSILON || beat >= sequence.endBeat - EPSILON) break;
        notes.push(Object.freeze({ ...note, position: beat, iteration: cycle, judgement: judgements.get(cycle)?.get(note.id) ?? null }));
      }
      const firstBar = Math.max(0, Math.floor((start - base) / exercise.barBeats));
      const lastBar = Math.min(sequence.bars.length - 1, Math.floor((end - base) / exercise.barBeats));
      for (let barIndex = firstBar; barIndex <= lastBar; barIndex++) {
        const barStart = base + barIndex * exercise.barBeats;
        if (barStart > end || barStart + exercise.barBeats < start || barStart >= sequence.endBeat) continue;
        // Denominator-note beats, expressed in the transport's quarter-note units.
        const subdivision = exercise.beatUnit / 4;
        for (let offset = 0; offset < exercise.barBeats - EPSILON; offset += subdivision) {
          const beat = barStart + offset;
          if (beat < start - EPSILON || beat > end + EPSILON || beat >= sequence.endBeat) continue;
          const kind = offset < EPSILON ? 'bar' : Math.abs(offset / exercise.beatUnit - Math.round(offset / exercise.beatUnit)) < EPSILON ? 'beat' : 'subdivision';
          boundaries.push(Object.freeze({ position: beat, kind, bar: sequence.bars[barIndex] + 1 }));
        }
      }
    }
    renderer.update(Object.freeze({
      lanes: Object.freeze(visibleLanes.slice()),
      notes: Object.freeze(notes),
      boundaries: Object.freeze(boundaries),
      unmatched: Object.freeze(visibleUnmatched),
      position,
      playback,
      timeline: Object.freeze({ bars: Object.freeze(sequence.bars.slice()), origin: sequence.origin, endBeat: sequence.endBeat, iteration: Math.max(0, Math.floor((position - sequence.origin) / length)) }),
      visibleRange: Object.freeze({ start, end }),
    }));
  }

  /** @param {number[]} bars
   * @param {number | undefined} endBeat
   */
  function setSequence(bars, endBeat) {
    if (!exercise) throw new Error('Exercise required');
    const count = exercise.barCount;
    const chosen = bars.length ? [...bars] : Array.from({ length: count }, (_, index) => index);
    if (chosen.some((bar, index) => !Number.isInteger(bar) || bar < 0 || bar >= count || (index > 0 && bar <= chosen[index - 1]))) {
      throw new Error('Highway bars must be unique, sorted exercise bar indices');
    }
    const barBeats = exercise.barBeats;
    const origin = chosen[0] * barBeats;
    /** @type {Timeline['notes']} */
    const notes = [];
    const indices = new Map(chosen.map((bar, index) => [bar, index]));
    for (const note of exercise.notes) {
      // Rust supplies the same written-onset bar membership used by scoring;
      // the original MIDI beat still determines the actual expected timing.
      const bar = note.sourceBar;
      const index = indices.get(bar);
      if (index !== undefined) notes.push(Object.freeze({ ...note, sequenceBeat: index * barBeats + note.beat - bar * barBeats }));
    }
    notes.sort((a, b) => a.sequenceBeat - b.sequenceBeat);
    const limit = endBeat ?? origin + chosen.length * barBeats;
    if (!Number.isFinite(limit) || limit <= origin) throw new Error('Highway end beat must follow its origin');
    sequence = { bars: chosen, origin, endBeat: limit, notes };
    return sequence;
  }

  try { renderer.initialize(); }
  catch (error) {
    try { renderer.dispose(); } catch (disposeError) { console.error('Presentation disposal failed', disposeError); }
    throw error;
  }
  return Object.freeze({
    /** @param {Exercise} source */
    setExercise(source) {
      assertActive();
      const unsupported = [...new Set(source.hits.map(hit => hit.instrument).filter(instrument => !instrumentLanes.has(instrument)))];
      if (unsupported.length) throw new Error(`Highway does not support: ${unsupported.join(', ')}`);
      const barBeats = source.meter_numerator * 4 / source.meter_denominator;
      if (!Number.isFinite(barBeats) || barBeats <= 0 || !Number.isFinite(source.ticks_per_beat) || source.ticks_per_beat <= 0) throw new Error('Invalid exercise timing');
      const notes = source.hits.map(hit => {
        if (!hit.id || !Number.isFinite(hit.beat) || hit.beat < 0 || !Number.isInteger(hit.source_bar) || hit.source_bar < 0) throw new Error('Invalid expected note');
        return Object.freeze({
          id: hit.id, instrument: hit.instrument, lane: instrumentLanes.get(hit.instrument) ?? '', articulation: hit.articulation ?? 'unspecified', beat: hit.beat, sourceBar: hit.source_bar,
          duration: hit.raw_duration_ticks == null ? null : hit.raw_duration_ticks / source.ticks_per_beat,
          hand: hit.hand, accent: Boolean(hit.accent), grace: Boolean(hit.grace),
        });
      });
      const lastBeat = notes.reduce((last, note) => Math.max(last, note.beat), 0);
      // Match the application's exercise_beats convention at bar boundaries.
      exercise = { barBeats, beatUnit: 4 / source.meter_denominator, barCount: Math.max(1, Math.ceil((lastBeat + 0.001) / barBeats)), notes };
      judgements = new Map();
      unmatched = [];
      position = setSequence([], undefined).origin;
      update();
    },
    /** @param {number[]} bars
     * @param {number} [endBeat]
     */
    setTimeline(bars, endBeat) { assertActive(); if (!exercise) throw new Error('Exercise required'); setSequence(bars, endBeat); update(); },
    /** @param {number} beat
     * @param {number | null} [tempoBpm]
     */
    setPosition(beat, tempoBpm = playback.tempoBpm) {
      assertActive();
      if (!Number.isFinite(beat)) throw new Error('Invalid playback position');
      position = beat;
      playback = Object.freeze({ ...playback, tempoBpm });
      update();
    },
    /** @param {string} phase
     * @param {number | null} tempoBpm
     * @param {Omit<Playback, 'phase' | 'tempoBpm'>} [details]
     */
    setPlayback(phase, tempoBpm, details = {}) { assertActive(); playback = Object.freeze({ ...details, phase, tempoBpm }); update(); },
    // The application must provide the matched repetition for early hits near
    // loop boundaries. This avoids colouring the same expected ID in every
    // visible repetition, without asking the renderer to match notes.
    /** @param {Judgement[]} results
     * @param {number} iteration
     */
    setJudgements(results, iteration = sequence && exercise ? Math.max(0, Math.floor((position - sequence.origin) / (sequence.bars.length * exercise.barBeats))) : 0) {
      assertActive();
      if (!sequence || !exercise) throw new Error('Exercise required');
      /** @type {Map<number, Map<string, string>>} */
      const next = new Map();
      /** @type {typeof unmatched} */
      const nextUnmatched = [];
      for (const [index, result] of results.entries()) {
        const cycle = result.iteration ?? iteration;
        if (!Number.isInteger(cycle) || cycle < 0) throw new Error('Invalid judgement iteration');
        if (!result.expected_id) {
          const actualBeat = result.actual_beat;
          if (typeof actualBeat !== 'number' || !Number.isFinite(actualBeat) || !['extra', 'wrong_instrument'].includes(result.judgement)) continue;
          const sourceBar = Math.floor((actualBeat + EPSILON) / exercise.barBeats);
          const selectedIndex = sequence.bars.indexOf(sourceBar);
          if (selectedIndex < 0) continue;
          const offset = actualBeat - sourceBar * exercise.barBeats;
          nextUnmatched.push({
            id: result.event_id ?? `unmatched:${cycle}:${result.actual_beat}:${index}`,
            instrument: result.instrument,
            lane: instrumentLanes.get(result.instrument) ?? 'other',
            judgement: result.judgement,
            iteration: cycle,
            position: sequence.origin + cycle * sequence.bars.length * exercise.barBeats + selectedIndex * exercise.barBeats + offset,
          });
          continue;
        }
        let byCycle = next.get(cycle);
        if (!byCycle) { byCycle = new Map(); next.set(cycle, byCycle); }
        byCycle.set(result.expected_id, result.judgement === 'on_time' ? result.excellent ? 'perfect' : 'good' : result.judgement);
      }
      judgements = next;
      unmatched = nextUnmatched;
      update();
    },
    /** @param {number} past
     * @param {number} future
     */
    setVisibleSpan(past, future) {
      assertActive();
      if (!Number.isFinite(past) || !Number.isFinite(future) || past < 0 || future <= 0) throw new Error('Invalid visible span');
      before = past; after = future; update();
    },
    /** @param {import('./presentation-types.mjs').Viewport} viewport */
    resize(viewport) { assertActive(); renderer.resize(Object.freeze({ ...viewport })); },
    dispose() { if (disposed) return; disposed = true; renderer.dispose(); exercise = null; sequence = null; judgements.clear(); unmatched = []; },
  });
}

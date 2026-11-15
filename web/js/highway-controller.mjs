// Renderer-independent highway presentation. The application supplies beats and
// judgements; this module neither reads a clock nor changes musical state.
const EPSILON = 1e-9;

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
 * Renderer contract: initialize(), update(presentation), resize(viewport),
 * dispose(). A renderer receives detached, immutable presentation snapshots.
 * It must not be given the transport or any scoring callbacks.
 */
export function createHighwayController(renderer, lanes = [{ instrument: 'snare', label: 'SNARE' }]) {
  const configuredLanes = lanes.map(lane => Object.freeze({ ...lane }));
  const instruments = new Set(configuredLanes.map(lane => lane.instrument));
  let exercise = null;
  let sequence = null;
  let position = 0;
  let before = 2;
  let after = 6;
  let judgements = new Map();
  let playback = Object.freeze({ phase: 'idle', tempoBpm: null });
  let disposed = false;

  function assertActive() {
    if (disposed) throw new Error('Highway controller has been disposed');
  }

  function update() {
    if (!exercise || !sequence) return;
    const start = position - before;
    const end = position + after;
    const notes = [];
    const boundaries = [];
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
        // Quarter-beat subdivisions, in quarter-note beat units. Include the
        // bar line separately so unusual time signatures retain exact bars.
        for (let offset = 0; offset < exercise.barBeats - EPSILON; offset += 0.25) {
          const beat = barStart + offset;
          if (beat < start - EPSILON || beat > end + EPSILON || beat >= sequence.endBeat) continue;
          const kind = offset < EPSILON ? 'bar' : Math.abs(offset - Math.round(offset)) < EPSILON ? 'beat' : 'subdivision';
          boundaries.push(Object.freeze({ position: beat, kind, bar: sequence.bars[barIndex] + 1 }));
        }
      }
    }
    renderer.update(Object.freeze({
      lanes: Object.freeze(configuredLanes.slice()),
      notes: Object.freeze(notes),
      boundaries: Object.freeze(boundaries),
      position,
      playback,
      timeline: Object.freeze({ bars: Object.freeze(sequence.bars.slice()), origin: sequence.origin, endBeat: sequence.endBeat, iteration: Math.max(0, Math.floor((position - sequence.origin) / length)) }),
      visibleRange: Object.freeze({ start, end }),
    }));
  }

  function setSequence(bars, endBeat) {
    const count = exercise.barCount;
    const chosen = bars.length ? [...bars] : Array.from({ length: count }, (_, index) => index);
    if (chosen.some((bar, index) => !Number.isInteger(bar) || bar < 0 || bar >= count || (index > 0 && bar <= chosen[index - 1]))) {
      throw new Error('Highway bars must be unique, sorted exercise bar indices');
    }
    const barBeats = exercise.barBeats;
    const origin = chosen[0] * barBeats;
    const notes = [];
    const indices = new Map(chosen.map((bar, index) => [bar, index]));
    for (const note of exercise.notes) {
      // Scoring assigns source notes to bars using the written onset, while
      // retaining the original MIDI beat for their actual expected timing.
      const bar = Math.floor((note.notationBeat + 0.000001) / barBeats);
      const index = indices.get(bar);
      if (index !== undefined) notes.push(Object.freeze({ ...note, sequenceBeat: index * barBeats + note.beat - bar * barBeats }));
    }
    notes.sort((a, b) => a.sequenceBeat - b.sequenceBeat);
    const limit = endBeat ?? origin + chosen.length * barBeats;
    if (!Number.isFinite(limit) || limit <= origin) throw new Error('Highway end beat must follow its origin');
    sequence = { bars: chosen, origin, endBeat: limit, notes };
  }

  try { renderer.initialize(); }
  catch (error) {
    try { renderer.dispose(); } catch (disposeError) { console.error('Presentation disposal failed', disposeError); }
    throw error;
  }
  return Object.freeze({
    setExercise(source) {
      assertActive();
      const unsupported = [...new Set(source.hits.map(hit => hit.instrument).filter(instrument => !instruments.has(instrument)))];
      if (unsupported.length) throw new Error(`Highway does not support: ${unsupported.join(', ')}`);
      const barBeats = source.meter_numerator * 4 / source.meter_denominator;
      if (!Number.isFinite(barBeats) || barBeats <= 0 || !Number.isFinite(source.ticks_per_beat) || source.ticks_per_beat <= 0) throw new Error('Invalid exercise timing');
      const notes = source.hits.map(hit => {
        if (!hit.id || !Number.isFinite(hit.beat) || hit.beat < 0) throw new Error('Invalid expected note');
        return Object.freeze({
          id: hit.id, instrument: hit.instrument, beat: hit.beat, notationBeat: hit.notation_beat ?? hit.beat,
          duration: hit.raw_duration_ticks == null ? null : hit.raw_duration_ticks / source.ticks_per_beat,
          hand: hit.hand, accent: Boolean(hit.accent),
        });
      });
      const lastBeat = notes.reduce((last, note) => Math.max(last, note.beat), 0);
      // Match the application's exercise_beats convention at bar boundaries.
      exercise = { barBeats, barCount: Math.max(1, Math.ceil((lastBeat + 0.001) / barBeats)), notes };
      judgements = new Map();
      setSequence([], undefined);
      position = sequence.origin;
      update();
    },
    setTimeline(bars, endBeat) { assertActive(); if (!exercise) throw new Error('Exercise required'); setSequence(bars, endBeat); update(); },
    setPosition(beat, tempoBpm = playback.tempoBpm) {
      assertActive();
      if (!Number.isFinite(beat)) throw new Error('Invalid playback position');
      position = beat;
      playback = Object.freeze({ ...playback, tempoBpm });
      update();
    },
    setPlayback(phase, tempoBpm, details = {}) { assertActive(); playback = Object.freeze({ ...details, phase, tempoBpm }); update(); },
    // The application must provide the matched repetition for early hits near
    // loop boundaries. This avoids colouring the same expected ID in every
    // visible repetition, without asking the renderer to match notes.
    setJudgements(results, iteration = sequence ? Math.max(0, Math.floor((position - sequence.origin) / (sequence.bars.length * exercise.barBeats))) : 0) {
      assertActive();
      const next = new Map();
      for (const result of results) {
        if (!result.expected_id) continue;
        const cycle = result.iteration ?? iteration;
        if (!Number.isInteger(cycle) || cycle < 0) throw new Error('Invalid judgement iteration');
        if (!next.has(cycle)) next.set(cycle, new Map());
        next.get(cycle).set(result.expected_id, result.judgement === 'on_time' ? result.excellent ? 'perfect' : 'good' : result.judgement);
      }
      judgements = next;
      update();
    },
    setVisibleSpan(past, future) {
      assertActive();
      if (!Number.isFinite(past) || !Number.isFinite(future) || past < 0 || future <= 0) throw new Error('Invalid visible span');
      before = past; after = future; update();
    },
    resize(viewport) { assertActive(); renderer.resize(Object.freeze({ ...viewport })); },
    dispose() { if (disposed) return; disposed = true; renderer.dispose(); exercise = null; sequence = null; judgements.clear(); },
  });
}

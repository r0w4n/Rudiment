// @ts-check
import { createHighwayController } from './highway-controller.mjs';

// Optional, synchronous tap into existing application events. No timers,
// transport subscriptions, DOM observers or independent exercise state.
/** @typedef {import('./presentation-types.mjs').Renderer} Renderer */
/** @typedef {import('./presentation-types.mjs').Lane} Lane */
/** @typedef {import('./presentation-types.mjs').Snapshot} Snapshot */
/** @typedef {import('./presentation-types.mjs').Configuration} Configuration */
/** @typedef {import('./presentation-types.mjs').Judgement} Judgement */
/** @typedef {ReturnType<typeof createHighwayController>} Controller */
/** @param {() => {beat: number; tempoBpm: number} | null} currentPosition
 * @param {(error: unknown) => void} onError
 */
export function createPresentationHost(currentPosition, onError = () => {}) {
  /** @type {(() => Snapshot) | null} */
  let snapshot = null;
  /** @type {Controller | null} */
  let active = null;
  let timelineSignature = '';

  function dispose() {
    const previous = active;
    active = null;
    timelineSignature = '';
    if (previous) {
      try { previous.dispose(); } catch (error) { console.error('Presentation disposal failed', error); }
    }
  }

  // A faulty optional renderer must never interrupt audio or MIDI processing.
  /** @param {(controller: Controller) => void} callback */
  function forward(callback) {
    if (!active) return;
    try { callback(active); }
    catch (error) {
      dispose();
      console.error('Presentation renderer detached', error);
      try { onError(error); } catch (notificationError) { console.error('Presentation error notification failed', notificationError); }
    }
  }

  /** @param {Controller} controller
   * @param {Configuration} config
   */
  function configure(controller, config) {
    const signature = JSON.stringify([config.bars, config.endBeat]);
    if (signature !== timelineSignature) {
      controller.setTimeline(config.bars, config.endBeat);
      timelineSignature = signature;
    }
    controller.setPlayback(config.phase, config.tempoBpm, { loops: config.loops, repeatUntilGood: config.repeatUntilGood, barAttempt: config.barAttempt });
    const transport = currentPosition();
    controller.setPosition(transport?.beat ?? config.position, transport?.tempoBpm ?? config.tempoBpm);
  }

  return Object.freeze({
    /** @param {() => Snapshot} provider */
    bind(provider) { snapshot = provider; },
    unbind() { snapshot = null; dispose(); },
    isActive() { return active !== null; },
    /** @param {Renderer} renderer
     * @param {readonly Lane[]} [lanes]
     */
    mount(renderer, lanes) {
      if (!snapshot) throw new Error('Application presentation state is not ready');
      if (active) throw new Error('A presentation renderer is already mounted');
      const state = snapshot();
      if (!state?.exercise || !state.config) throw new Error('Application presentation state is unavailable');
      const controller = createHighwayController(renderer, lanes);
      try {
        controller.setExercise(state.exercise);
        controller.setVisibleSpan(6, 6);
        configure(controller, state.config);
        controller.setJudgements(state.judgements);
      } catch (error) {
        try { controller.dispose(); } catch (disposeError) { console.error('Presentation disposal failed', disposeError); }
        timelineSignature = '';
        throw error;
      }
      active = controller;
      return Object.freeze({
        /** @param {import('./presentation-types.mjs').Viewport} viewport */
        resize(viewport) { if (active === controller) forward(current => current.resize(viewport)); },
        dispose() { if (active === controller) dispose(); },
      });
    },
    exerciseChanged() {
      const provider = snapshot;
      if (!provider) return;
      forward(controller => {
        const state = provider();
        controller.setExercise(state.exercise);
        timelineSignature = '';
        configure(controller, state.config);
        controller.setJudgements(state.judgements);
      });
    },
    /** @param {Configuration} config */
    configure(config) { forward(controller => configure(controller, config)); },
    /** @param {Judgement[]} results */
    judgements(results) { forward(controller => controller.setJudgements(results)); },
    /** @param {number} beat
     * @param {number} tempoBpm
     */
    position(beat, tempoBpm) { forward(controller => controller.setPosition(beat, tempoBpm)); },
  });
}

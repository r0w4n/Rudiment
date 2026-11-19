// The Rust/WASM presentation snapshot contract. These types have no runtime module.
export type Instrument =
  | 'snare'
  | 'kick'
  | 'closed_hat'
  | 'open_hat'
  | 'pedal_hat'
  | 'ride'
  | 'crash'
  | 'tom'
  | 'other_percussion'
  | 'unknown';

export type Assessment =
  | 'perfect'
  | 'good'
  | 'on_time'
  | 'early'
  | 'late'
  | 'missed'
  | 'extra'
  | 'wrong_instrument';

export interface ExpectedHit {
  id: string;
  beat: number;
  source_bar: number;
  instrument: Instrument;
  notation_beat?: number;
  raw_duration_ticks?: number | null;
  articulation?: string;
  hand?: string;
  accent?: boolean;
  grace?: boolean;
}

export interface Exercise {
  meter_numerator: number;
  meter_denominator: number;
  ticks_per_beat: number;
  hits: ExpectedHit[];
}

export interface Judgement {
  expected_id?: string | null;
  actual_beat?: number | null;
  event_id?: string | null;
  iteration?: number | null;
  instrument: Instrument;
  judgement: Assessment;
  excellent?: boolean;
}

export interface Lane {
  instrument: string;
  label: string;
  instruments?: readonly string[];
}

export interface SourceNote {
  id: string;
  instrument: Instrument;
  lane: string;
  articulation: string;
  beat: number;
  sourceBar: number;
  duration: number | null;
  hand?: string;
  accent: boolean;
  grace: boolean;
}

export interface Note extends SourceNote {
  sequenceBeat: number;
}

export interface Timeline {
  bars: number[];
  origin: number;
  endBeat: number;
  notes: Note[];
}

export interface Playback {
  phase: string;
  tempoBpm: number | null;
  loops?: number;
  repeatUntilGood?: boolean;
  barAttempt?: number;
}

export interface Presentation {
  lanes: readonly Lane[];
  notes: readonly (Note & { position: number; iteration: number; judgement: string | null })[];
  boundaries: readonly { position: number; kind: string; bar: number }[];
  unmatched: readonly { id: string; instrument: Instrument; lane: string; judgement: Assessment; iteration: number; position: number }[];
  position: number;
  playback: Playback;
  timeline: { bars: readonly number[]; origin: number; endBeat: number; iteration: number };
  visibleRange: { start: number; end: number };
}

export interface Viewport {
  width: number;
  height: number;
}

export interface Renderer {
  initialize(): void;
  update(presentation: Presentation): void;
  resize(viewport: Viewport): void;
  dispose(): void;
}

export interface Configuration {
  bars: number[];
  endBeat: number;
  position: number;
  phase: string;
  tempoBpm: number;
  loops: number;
  repeatUntilGood: boolean;
  barAttempt: number;
}

export interface Snapshot {
  exercise: Exercise;
  config: Configuration;
  judgements: Judgement[];
}

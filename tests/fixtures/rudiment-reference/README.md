# Rudiment — historically sourced notation/MIDI fixtures

## Important provenance distinction

**Source (public domain):** Charles Stewart Ashworth, *A New, Useful and Complete System of Drum Beating* (1812), original historical scan: https://www.benjaminwaterson.com/wp-content/uploads/2023/10/1812-A-New-Useful-and-Complete-System-of-Drum-Beating-Ashworth.pdf

**These 11 examples are new, modernised practice exercises using named historical rudiments, NOT exact transcriptions or facsimiles of the source**. The aim is consistent, deterministic reference material for rendering and MIDI-parser tests. Do not label these as a note-for-note recreation of Ashworth's 1812 pages.

The score SVG/PNG files and manifest in this directory, plus the matching built-in MIDI files under `web/public/exercises/rudiments`, are independently generated for reuse. The image assets are test-only fixtures and are not served by Rudiment. No exclusive rights are asserted by this package; confirm your own distribution rights if incorporating them into a commercial product.

## Content

- Ten examples with labels drawn from *exact historical names*, printed pages 3–5; four bars each, one bar per line.
- One original eight-bar advanced study inspired by the complex historical *The Drummers Call* (printed p. 6). This advanced example is not a transcription of the piece.
- Every `*.mid` is SMF format 1, drum channel 10, General MIDI snare 38, 4/4, 72 BPM, PPQ 960.
- For **every MIDI note-on, including grace strikes**, a same-tick MIDI **lyric meta event (`FF 05`) contains `R` or `L`**. It appears immediately before the note-on in the snare track.
- The `manifest.json` is the *ground-truth timing and notation model* used for both renders; `beat` is the in-bar zero-based quarter-note offset, `duration` is in quarter-note beats, `grace` marks ornament attacks, and `tuplet` identifies triplet units.
- For bar-initial ornaments, the MIDI moves the first grace group and its main note by 0.17 quarter beats so grace notes remain inside the measure. This means the first primary note may be slightly after the barline. Refer to event JSON for notation-level timing.

## Why these aren't literal historical facsimiles

The 1812 originals use paired upper (left hand) and lower (right hand) note positioning with historic rhythmic conventions. These fixtures adapt them into modern single-line snare notation with explicit hand letters. Hand pattern identities were checked against the original, but original rhythmic values in that historic notation were **not independently transcribed and verified**. The scored patterns were independently written to be internally consistent and instructive.

## Source to contemporary pattern mapping

| Reference image | Original printed page | Rudiment or historical label | Modernisation check |
|---|---:|---|---|
| 01 | 3 | Roll | Consecutive paired strokes, hands alternate |
| 02 | 4 | Mother or 5 S. Roll | Two double strokes + one finishing tap |
| 03 | 3 | 7 Stroke Roll | Six inner strokes + terminal accent |
| 04 | 3 | 9 Stroke Roll | Eight inner strokes + terminal accent |
| 05 | 4 | Hard Flams | One quiet opposing grace and one accented main strike |
| 06 | 4 | Single Paradiddle | RLRR / LRLL |
| 07 | 4 | Double Paradiddle | RLRLRR / LRLRLL |
| 08 | 4 | Flam Paradiddle | Paradiddle plus opposing-hand grace on group head |
| 09 | 5 | Single Drag | Two quiet grace strokes ahead of a tap |
| 10 | 5 | Single Rotamacue | Historical spelling; two-stroke grace + triplet construction |
| 11 | 6 | The Drummers Call — Advanced Study | Independently authored complex étude, not original call |

## Functional validation

All tracks were reloaded from their output `.mid` files. For each track the number of lyric events equals the number of note-on attacks, and the timestamp multiset of lyric events is identical to the timestamp multiset of attacks. All struck pitches are GM acoustic snare 38.

## Limits for app testing

- Treat each PNG as a reference for **rendering the accompanying manifest model**, rather than demanding the app re-engrave a raw MIDI with identical notehead durations. Ordinary MIDI cannot encode beaming, rhythm-grouping, or hand annotations as standard percussion semantics.
- Rudiment can parse the deliberately defined lyric convention, but other MIDI apps may ignore these meta-events. Treat it as a vendor-specific convention, not an interoperable standard.
- A note-on for the grace stroke is deliberately audible, and therefore increases total MIDI attack count over the principal-stroke count.
- These images are purpose-built to test one-bar-per-line layout, not to demonstrate exact visual identity with the 1812 scan.

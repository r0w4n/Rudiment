# Third-party notices

Exact resolved versions are recorded in `package-lock.json` and `Cargo.lock`.

- **VexFlow 5.0.0** — MIT. Music engraving. https://github.com/0xfe/vexflow
- **Tone.js 15.1.22** — MIT. Web Audio scheduling. https://github.com/Tonejs/Tone.js
- **WEBMIDI.js 3.3.1** — Apache-2.0. Web MIDI adapter. https://github.com/djipco/webmidi
- **idb 8.0.4** — ISC. IndexedDB wrapper. https://github.com/jakearchibald/idb
- **alphaTab 1.9.0** — MPL-2.0. Development-only renderer evaluation; not in the production bundle. Its reproduction page preserves the rendered attribution. https://www.alphatab.net/
- **Leptos 0.8.23** — MIT. Rust/Wasm UI. https://github.com/leptos-rs/leptos
- **Lucide Leptos 3.63.0** — MIT. Consistent interface icon components. https://github.com/RustForWeb/lucide
- **midly 0.5.3** — MIT / Apache-2.0. Standard MIDI File parser. https://github.com/kovaxis/midly

## Acoustic snare recordings

Four velocity layers are derived unchanged from the **Acoustic Snare** instrument in Keyboardia, sourced from Virtuosity Drums by Versilian Studios and Karoryfer Samples. The recordings are dedicated under **CC0 1.0 Universal**. Upstream licence manifest: https://github.com/adewale/keyboardia/blob/main/app/public/instruments/LICENSE.md

Vendored files and upstream names are listed in `web/public/audio/ATTRIBUTION.md`. No commercial kit samples are included.

## Historical rudiment reference set

The 11 built-in reference exercises are modern teaching adaptations based on rudiment names in Charles Stewart Ashworth's *A New, Useful and Complete System of Drum Beating* (1812), a public-domain historical work. The supplied MIDI, manifest and reference PNG/SVG engravings are independently generated modern exercises and are not facsimiles or verified note-for-note transcriptions. Their full provenance and limitations are preserved in `tests/fixtures/rudiment-reference/README.md`.

# Rudiment acoustic snare samples

These four velocity layers are unmodified acoustic snare recordings from Keyboardia's `acoustic-snare` instrument:

| Rudiment file | Upstream file |
| --- | --- |
| `snare-soft.m4a` | `source-0004.m4a` |
| `snare-medium.m4a` | `source-0012.m4a` |
| `snare-hard.m4a` | `source-0024.m4a` |
| `snare-accent.m4a` | `source-0034.m4a` |

The matching `.wav` files are mono, 44.1 kHz PCM conversions of those same
CC0 recordings, included for browsers whose embedded Web Audio implementation
does not reliably decode AAC/M4A.

Source: Virtuosity Drums by Versilian Studios and Karoryfer Samples (mid mic), repackaged by Keyboardia.

Licence: **CC0 1.0 Universal (Public Domain)**.

- Upstream licence manifest: https://github.com/adewale/keyboardia/blob/main/app/public/instruments/LICENSE.md
- Upstream instrument directory: https://github.com/adewale/keyboardia/tree/main/app/public/instruments/acoustic-snare
- CC0 legal code: https://creativecommons.org/publicdomain/zero/1.0/legalcode

## Sampled metronome

These six percussion recordings are from the Versilian Community Sample
Library (VCSL), distributed under **CC0 1.0 Universal**. The files were
selected from the VCSL-for-Estuary mirror after comparing their velocity
layers, attack and decay; the mirror describes itself as a formatted version
of VCSL.

| Bundled file | Original filename | Source recording |
| --- | --- | --- |
| `woodblock-accent.wav` | `wood_click_ff.wav` | [woodblock/wood_click_ff.wav](https://github.com/carltesta/vcsl_for_estuary/blob/main/woodblock/wood_click_ff.wav) |
| `woodblock-regular.wav` | `wood_click_pp_rr1.wav` | [woodblock/wood_click_pp_rr1.wav](https://github.com/carltesta/vcsl_for_estuary/blob/main/woodblock/wood_click_pp_rr1.wav) |
| `click-accent.wav` | `01_Claves1_Hit_v3_rr1_Mid.wav` | [clave/01_Claves1_Hit_v3_rr1_Mid.wav](https://github.com/carltesta/vcsl_for_estuary/blob/main/clave/01_Claves1_Hit_v3_rr1_Mid.wav) |
| `click-regular.wav` | `00_Claves1_Hit_v2_rr1_Mid.wav` | [clave/00_Claves1_Hit_v2_rr1_Mid.wav](https://github.com/carltesta/vcsl_for_estuary/blob/main/clave/00_Claves1_Hit_v2_rr1_Mid.wav) |
| `cowbell-accent.wav` | `Cowbell1_Hit_v4_rr1_Mid.wav` | [cowbell/Cowbell1_Hit_v4_rr1_Mid.wav](https://github.com/carltesta/vcsl_for_estuary/blob/main/cowbell/Cowbell1_Hit_v4_rr1_Mid.wav) |
| `cowbell-regular.wav` | `Cowbell1_Hit_v2_rr1_Mid.wav` | [cowbell/Cowbell1_Hit_v2_rr1_Mid.wav](https://github.com/carltesta/vcsl_for_estuary/blob/main/cowbell/Cowbell1_Hit_v2_rr1_Mid.wav) |

The original velocity layers are retained: `ff`/`pp` for woodblock, VCSL
velocity layers `v3`/`v2` for claves, and `v4`/`v2` for cowbell. The
recording-level contributor is not identified in the sample filenames or
library README; VCSL is credited to Versilian Studios LLC and community
contributors. The Estuary mirror was formatted by Carl Testa.

Processing was offline and did not add EQ or pitch changes. Each stereo
recording was downmixed to mono PCM, trimmed to its first clear attack with
2 ms of pre-roll, and capped at 160 ms (claves) or 180 ms (woodblock and
cowbell). A 1 ms fade-in and 15 ms fade-out preserve a clean onset and avoid
truncation clicks. Files remain at 44.1 kHz, are encoded as 16-bit PCM, and
are peak-normalized per instrument/layer for consistent delivery level and a
clear accent-to-regular contrast. The six bundled files total about 91 KB.

- VCSL-for-Estuary source repository: https://github.com/carltesta/vcsl_for_estuary
- Original VCSL repository and CC0 statement: https://github.com/sgossner/VCSL
- VCSL license file: https://github.com/sgossner/VCSL/blob/master/LICENSE
- CC0 legal code: https://creativecommons.org/publicdomain/zero/1.0/legalcode

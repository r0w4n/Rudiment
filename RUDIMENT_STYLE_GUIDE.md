# Rudiment — UI Style Guide

**Version:** 1.0 (screenshot-derived baseline)  
**Date:** 14 November 2026  
**Status:** Proposed specification for implementation and review  
**Audience:** Product designers, frontend developers, Codex and QA

> **Evidence and precision:** This guide is based on five supplied desktop screenshots of Rudiment (MIDI library, Sound & drums drawer, practice-bar popover, metronome popover, and the main performance screen). Visual measurements and colour values are **approximations**, not extracted from the application’s CSS, design files or component source. Preserve existing implementation tokens where they are demonstrably intentional, accessible and consistent; do not replace exact values merely to match a screenshot estimate. Behaviour not visible in screenshots is identified as a **recommendation** rather than an observed fact.

---

## 1. Design intent

Rudiment is a focused musical-practice application rather than a generic analytics dashboard. Its interface should be calm, tactile, legible, responsive, and exceptionally predictable during rhythmic performance.

### Core principles

1. **Music is primary.** The notation is the main content; surrounding controls should be visually quieter.
2. **Dark frame, warm score.** Use dark, slightly green-biased chrome around a warm, paper-like notation surface.
3. **One accent language.** Use mint/teal for primary actions, current selection, focus and related affordances; distinguish those roles using fill, stroke and intensity.
4. **Consistent components.** Dialogs, drawers and popovers should share underlying tokens even when their layouts differ.
5. **Clear live feedback.** Timing and note judgments must be understandable immediately, without depending solely on colour.
6. **No performance disruption.** Menus, transitions and feedback must not interfere with musical timing or MIDI handling.
7. **Compact without being cramped.** Use measured spacing and predictable control dimensions.

### Scope

Included: app shell, top toolbar, score canvas, notation status, exercise library, configuration drawer, practice-bars and metronome popovers, form controls, bottom action bar, timing legend, responsive behaviour, accessibility, interaction and QA.

Not established by screenshots: mobile designs, exact font families, implementation frameworks, animation timings, historical charts, error states, every dropdown option, and all practice/performance states. The specifications for these are recommendations or placeholders for source-code confirmation.

---

## 2. Foundations and design tokens

### 2.1 Colour palette

The following values are **proposed visual matches**, not asserted existing CSS values. Centralise them in a single token source rather than hard-coding them across views.

| Token | Suggested hex | Purpose |
|---|---|---|
| `--color-app-bg` | `#0F1512` | Global dark app background |
| `--color-surface-dark` | `#151B18` | Library dialogs, drawers, header-adjacent surfaces |
| `--color-surface-dark-raised` | `#1A2420` | Dark cards, elevated dark controls |
| `--color-surface-dark-muted` | `#101714` | Dark input wells and inactive regions |
| `--color-border-dark` | `#33433D` | Dividers, card borders, outlined controls |
| `--color-text-on-dark` | `#F0F3EE` | Main text on dark backgrounds |
| `--color-text-on-dark-muted` | `#A2B1AA` | Secondary text, labels and captions |
| `--color-score-bg` | `#F4F0E6` | Primary notation/score background |
| `--color-surface-light` | `#F8F7F2` | Popovers and light UI panels |
| `--color-text-on-light` | `#283A32` | Text on warm/light surfaces |
| `--color-border-light` | `#C7D0CB` | Borders of light controls |
| `--color-accent` | `#37D9C1` | Primary action emphasis (bright mint) |
| `--color-accent-deep` | `#2D806F` | Active control borders, muted accent usage |
| `--color-accent-soft` | `#DCEDE7` | Selected light control background |
| `--color-accent-pale` | `#77D3C1` | Secondary accent on dark surfaces |
| `--color-feedback-perfect` | `#27D58B` | Perfect hit |
| `--color-feedback-good` | `#7BDE9C` | Good hit |
| `--color-feedback-early` | `#F59B4D` | Early hit |
| `--color-feedback-late` | `#A879D5` | Late hit |
| `--color-feedback-missed` | `#EA5454` | Missed note |
| `--color-overlay-scrim` | `rgba(0, 0, 0, 0.58)` | Modal backdrop; optionally apply background blur |

**Rules**

- Avoid pure black and pure white for broad interface surfaces; retain the warm/green bias.
- The primary CTA can be filled mint; secondary actions use restrained dark or light outlines.
- Use deep teal, not bright mint, for thin borders on the light score surface.
- Do not use orange, purple or red for routine navigation or decoration: reserve these for rhythmic feedback and genuine warnings/errors.
- Validate final text/foreground combinations rather than assuming a token pair meets contrast requirements.
- Preserve conventional black notation glyphs on the ivory score.

### 2.2 Typography

**Observed:** Large titles such as “MIDI library”, “Sound & drums”, and popover titles use a traditional serif-like face. Operational UI text, the app brand, labels and button text are primarily sans-serif.

**Recommended roles:**

| Role | Size at desktop | Weight | Typeface | Usage |
|---|---:|---:|---|---|
| Page/panel title | 24–28px | 600–700 | Serif | Library and major settings titles |
| Popover title | 15–17px | 600–700 | Serif | “Practice bars”, “Metronome” |
| App wordmark | 22–26px | 700 | Sans | Rudiment in header |
| Section heading | 15–16px | 600–700 | Sans | Metronome, Reference snare |
| Primary body | 14–15px | 400–500 | Sans | Descriptions and general content |
| Control label | 12–14px | 500–600 | Sans | Field names and toggles |
| Button text | 13–15px | 600–700 | Sans | Actions |
| Eyebrow / metadata | 11–12px | 600–700 | Sans, spaced caps | EXERCISES, CURRENT EXERCISE |
| Compact status | 10–12px | 600 | Sans | READY, BAR 1 / 80 |
| Musical notation | Engraving-driven | n/a | Music engraving font | Staves, rests, noteheads, percussion clef |

Typography rules:

- Restrict serif typography to **editorial headings**. In screenshots, some light-popover field values appear serif; standardise these to sans-serif unless there is a deliberate product reason to keep them.
- Use tabular numerals for BPM, bar counts, loop counts, scores and other changing numeric values.
- Use tracked uppercase lettering only for small section eyebrows and compact operational status; do not use it for full paragraphs.
- Avoid overusing bold text: hierarchy should come from spacing, size and alignment first.
- Select actual typefaces after checking the current font stack in the repository. Avoid adding a new font dependency without a clear benefit.

### 2.3 Spacing and sizing

Use a **4px base spacing unit**:

| Token | Value | Common application |
|---|---:|---|
| `--space-1` | 4px | Inline icon/text spacing |
| `--space-2` | 8px | Small control gaps |
| `--space-3` | 12px | Paired controls, card internals |
| `--space-4` | 16px | Standard component padding |
| `--space-5` | 20px | Medium internal spacing |
| `--space-6` | 24px | Sections and modal padding |
| `--space-8` | 32px | Major sections, large canvas insets |

Recommended sizing:

- Compact text input/select: **40–44px** visual height; increase interactive hit target where needed.
- Icon button: **44–48px** square target, even when artwork is smaller.
- Standard button: **44–48px** height; prominent start/restart actions may be **64–88px** high on the desktop score screen as shown.
- Modal close button: a **44px** target, with an unobtrusive icon and visible focus treatment.
- Do not use different component heights because the same element appears on a dark versus light surface.

### 2.4 Corners, borders and elevation

| Element | Suggested radius | Treatment |
|---|---:|---|
| Notation canvas | 20–24px | Warm background, minimal border |
| Main modal / drawer | 18–24px | Dark elevation; subtle outline |
| Exercise library card | 14–16px | Thin dark border |
| Popover | 14–18px | Light fill, subtle outline and soft shadow |
| Inputs / selects | 10–12px | 1px border |
| Buttons | 10–16px | Consistent variants |
| Pills / compact indicators | 999px | Reserved for genuine chips/status |

Avoid heavy box shadows on dark surfaces. Use restrained shadow only where spatial separation materially improves legibility, particularly for light popovers sitting over musical notation.

### 2.5 Motion

**Proposed:** 120–180ms for hover/focus feedback; 160–220ms for overlay entry; 100–160ms for pressed states. Prefer opacity and transform over layout-changing animation. Honour `prefers-reduced-motion`. No decorative animation may delay MIDI feedback, metronome scheduling, score progression, or note hit classification.

---

## 3. Layout system

### 3.1 App shell

**Observed:** A full-width dark app, with a top header, large ivory notation region, and a dedicated bottom action row.

Desktop structure:

1. **Header:** left brand mark and wordmark; central current-exercise title and supporting metadata; right MIDI connection indicator and icon actions (library, performance/stats, settings).
2. **Main score area:** warm ivory canvas, internally containing status, rendered notation and bottom bar/progress indicators.
3. **Bottom actions:** Restart (secondary), Start performance (dominant primary), and timing legend (supporting information).

Maintain a stable shell as menus open. Background content under modal overlays should not jump or reflow.

### 3.2 Header

- Align controls on a shared vertical centreline.
- Use modest, balanced left/right padding (~24–40px desktop).
- The current exercise block must remain visually secondary to the notation.
- Treat the MIDI-connection pill as a status indicator, **not** as the main CTA. Use an explicit connection state (“No MIDI drums detected”) with an appropriate icon or indicator.
- Group icon buttons consistently, using equal footprints and gaps.
- Badge counts, such as the library count, sit consistently on one corner of their icon.
- Do not allow the header to arbitrarily reorder or wrap icons at smaller widths.

### 3.3 Main notation surface

**Observed:** The score spans most of the viewport width, with four visible staff systems, small bar numbers, a central status region above the notation, and progress information along its bottom edge.

- Use `--color-score-bg` for the canvas; keep the staff and noteheads almost black.
- The score should be the largest visual area on the page.
- Keep the metronome control/status visually centred in the score header area.
- Reserve a stable zone for bar labels, playhead and score feedback; avoid movement when state changes from READY to playback.
- On long exercises, maintain predictable system layout and scrolling while keeping the current music readable.
- Preserve a consistent inset from the score edge, but let stave engraving determine horizontal/vertical positions.
- Avoid vertically stretching simple notation purely to fill the available canvas. Prefer intentional whitespace or a restrained maximum staff-system spacing.
- Ensure noteheads do not collide with staff lines, ledger lines, stems, flags, sticking labels or bar numbers.

### 3.4 Bottom performance actions

- Keep **Start performance** the unmistakable primary action: bright mint, dark text and a prominent play icon.
- Treat **Restart** as secondary, with dark filled/outlined styling.
- Position the performance timing legend to the right on wide screens; it should remain readable, not squeeze the primary action.
- Use spacing, not arbitrary separators, to distinguish these groups. A subtle divider is acceptable.
- Define a different primary label for running, paused and completed states rather than reusing “Start performance” ambiguously.

---

## 4. Component specifications

### 4.1 Button system

Define reusable button variants:

| Variant | Appearance | Examples |
|---|---|---|
| `primary` | Filled bright mint, dark high-contrast label | Start performance, Import MIDI |
| `secondary-dark` | Deep charcoal/green, outlined | Restart, Next |
| `secondary-light` | Warm/light background, green-muted border | All bars, Preview |
| `ghost` | Minimal border/fill until hover | Small secondary actions |
| `icon` | Square, centred icon | Folder, stats, settings, close |
| `danger` | Reserved red treatment for destructive action | Delete imported exercise (not shown) |

Every button needs default, hover, pressed, focus-visible, disabled and (where relevant) loading states. Do not use hover as the only cue for an action.

### 4.2 Form controls

**Observed:** settings use labelled checkboxes, numeric stepper for BPM, selects for count-in/subdivision/click timbre, slider for metronome volume, and a Preview action.

Specify a shared treatment for:

- Selects: full-width within their layout column, consistent chevron and internal padding.
- Checkboxes: mint selected state, clear hit target, associated clickable label.
- Steppers: minus/value/plus in one bounded control; numeric value centrally aligned; fixed width prevents resizing with the number.
- Sliders: clearly differentiated track, fill, thumb, and focus state.
- Field labels: always visible, outside the input, sufficiently contrasted.
- Helper text: directly below the relevant control or control group, not separated by unrelated content.
- Preview actions: a clear indication of what is being previewed, with no ambiguous persistent playback state.

### 4.3 MIDI library modal

**Observed screenshot characteristics:**

- Centred, large dark modal, roughly half the viewport width at desktop.
- Eyebrow “EXERCISES”, serif title “MIDI library”, and total counts beneath.
- Import MIDI action aligned toward the upper right of the content area.
- Two-tab segmented navigation: **Built-in** and **Your imports**, each displaying a count.
- Two-column grid of dark bordered exercise cards.
- Each card includes a title and brief description.
- Footer pagination with Previous on left, page count centred, Next on right.
- High-prominence close control at top right.

**Recommended implementation rules:**

- Desktop max-width approximately **900–960px**, constrained by viewport margins. Screenshot-only estimate; check actual CSS.
- Use consistent **24–32px** modal padding and ~12px card gutters.
- Set a **minimum card height** within each grid row; allow growth with content rather than cropping long names/descriptions.
- Cards should be keyboard actionable; hover, focus and selected states need distinct treatments.
- Selecting a card should have a clearly defined outcome (load exercise and close modal, or navigate to detail); do not conflate hover with selection.
- For imports, provide delete controls with clear affordance and confirmation for permanent destructive changes.
- Keep pagination attached to the bottom and preserve navigability if content scrolls.
- If a single page has fewer cards, keep the footer predictable without creating a huge dead area.
- Library tab controls should share state styling with other segmented controls.

### 4.4 Settings drawer — Sound & drums

**Observed:** a dark full-height drawer anchored to the right, backdrop dimming the score. It includes top eyebrow/title/close, section headings, stacked forms and a horizontal divider before Reference snare.

**Recommended:**

- Desktop width roughly **440–580px**, without exceeding the viewport. Source screenshot should determine exact existing width.
- Use a fixed header and independently scrollable settings content where vertical space is limited.
- Keep section margins and horizontal rules aligned to a single inset.
- Use one field hierarchy: section heading → field label → control → helper text.
- Make all controls usable without forcing the page behind the drawer to scroll.
- Do not close the drawer on routine setting changes; settings remain available for iterative adjustment.
- Include a visible close action; Escape closes it unless a nested confirmation requires handling.

### 4.5 Contextual popovers — shared component

**Observed:** The practice-bars selector opens a light popover near the lower-left score control. The metronome opens a larger light popover beneath the score's centre-top control. Both have a close X.

Build **one popover primitive** with content variants:

- `Popover / PracticeBars`: compact, 2-field From/To grid; “All bars” full-width action and contextual status text.
- `Popover / Metronome`: 2-column grouped controls (click during exercise, BPM, count-in, subdivision, timbre, volume); compact Preview and close controls.

Shared behaviour:

1. Anchor to the control that opened it, with a small visible gap.
2. Prefer logical position: metronome below its trigger; practice bars above the bar-range trigger.
3. Keep the popover within viewport bounds; flip or shift position near edges.
4. Close on X, Escape, outside click/tap, and opening another incompatible popover.
5. Moving the pointer away **must not be the sole automatic dismissal mechanism**; hover-based disappearance can cause accidental closure during adjustments.
6. Return focus to the trigger when appropriate on keyboard dismissal.
7. Settings changes update the shared underlying state used by the drawer; never create separate conflicting values.
8. Preserve existing playback semantics: settings that affect transport should be applied at clearly defined safe boundaries rather than silently changing audio mid-beat.

### 4.6 Exercise-range control

**Observed:** bottom-left `BAR 1 / 80` control opens a light practice-bars popover, with From, To and All bars.

- Use a distinct active state while its popover is open; do not style it like a disabled button.
- Validate `From <= To` and stay within exercise length.
- Explain whether a chosen range applies to a practice loop or a one-off performance.
- Surface the chosen range in a compact status label.
- Avoid ambiguity between “All bars” (selection action) and “Performance when played once” (mode description).

### 4.7 Metronome control

**Observed:** a very compact strip centred in the notation header with indicator/beat lights and READY, which opens a larger light settings popover.

- Keep the full strip clickable, not only the small indicators.
- Make the strip's interactive affordance visible using focus/hover/cursor styling.
- Distinguish metronome enabled/disabled, count-in, active beat and transport-ready states with icons/text as well as colour.
- Do not reduce beat visualization to sub-pixel indicators at mobile sizes.
- Show tempo primarily inside the metronome popover and settings drawer, as in the supplied design.
- Ensure Preview plays the currently selected click timbre at a predictable volume, without affecting running performance state.

### 4.8 Timing-result legend

Observed labels: **Perfect**, **Good**, **Early**, **Late**, **Missed** with green, light green, orange, purple and red markers.

- Preserve that classification order in legends, performance summaries and charts.
- Ensure colours are consistent across score note feedback and results.
- Provide a textual or shape/icon differentiation where possible for colour-vision accessibility.
- Treat the legend as informational, not as five actions.
- Do not let dynamic feedback repaint large portions of the score unnecessarily.

---

## 5. Overlay rules

### 5.1 Distinguish overlays by purpose

| Overlay | Position | Surface | Backdrop | Primary purpose |
|---|---|---|---|---|
| Modal | Centred | Dark | Dim + optional blur | Choosing a library exercise |
| Drawer | Right edge, full-height | Dark | Dim + optional blur | Persistent multi-section settings |
| Popover | Anchored to trigger | Warm/light | **No full-page modal backdrop** | Fast contextual configuration |
| Confirmation | Centred, compact | Consistent theme | Dim | Destructive/irreversible confirmation |

**Consistency rule:** Popovers should not unexpectedly block the entire screen or use modal behaviour just because they share form fields with settings. Dialogs and drawers should trap focus; nonmodal popovers should use appropriate focus behaviour without trapping the user unnecessarily.

### 5.2 Overlay stacking and lifecycle

- Exactly one primary modal/drawer should own the main backdrop at a time.
- Any nested confirmation appears above its parent, with a clear focus/stacking model.
- Opening the library while the settings drawer is active should follow a single well-defined rule (replace drawer, close first, or prohibit)—not allow coincident competing overlays.
- Closing an overlay should preserve exercise playback state unless a control explicitly says otherwise.
- Do not use close-button highlighting as an accidental selected state; reserve visible mint outlines for hover/focus where warranted.

---

## 6. Responsive behaviour

**Important:** Only desktop screenshots were provided. The following are proposed breakpoints/behaviours for validation, **not observed mobile layouts**.

| Viewport | Suggested behaviour |
|---|---|
| ≥ 1200px | Full desktop header, broad score canvas, horizontal bottom actions + legend, two-column library cards |
| 768–1199px | More compact header; score retains priority; bottom legend may move below main controls; library stays 2-column only when cards remain readable |
| 480–767px | Header switches to a clearly designed compact arrangement; library single-column; drawer width nearly full; popovers constrained to screen and may stack form fields |
| < 480px | Full-width safe-inset overlays, large accessible controls, notation tuned for narrow width, compact actions without horizontal overflow |

Requirements:

- Treat **available content width**, not only viewport width, as a component constraint.
- Never reorder icons unpredictably; collapse secondary actions into an explicit overflow menu if needed.
- A modal must stay fully reachable above an on-screen keyboard.
- Avoid horizontal scrolling in settings and library; notation may need a designed scrolling/zoom mode rather than squeezing notes illegibly.
- Keep popover actions, Close, and primary performance controls large enough for touch.
- Respect device safe areas and bottom browser navigation.
- Test both landscape and portrait orientations.

---

## 7. Accessibility and interaction quality

Target **WCAG 2.2 AA** for interface controls and text, subject to musical-notation-specific practical considerations.

- Aim for **4.5:1** contrast for normal text and **3:1** for large text and essential graphical control boundaries.
- Provide persistent `:focus-visible` styling that works on dark and light surfaces.
- All icon-only buttons need accessible names: “Open MIDI library”, “Open sound settings”, “Close metronome settings”, etc.
- All inputs have explicit labels, useful descriptions and validation messages.
- The metronome and playback affordances must have screen-reader-accessible state labels. Do not announce every rapid beat through a live region.
- Escape closes the current dismissible overlay, without also triggering unrelated keyboard shortcuts.
- Use robust click/touch targets; never depend on hover interactions for essential actions.
- Honour reduced motion and user zoom at 200% and 400% without losing functionality.
- Do not express note success/failure solely by marker colour. Pair with text/symbols in relevant detailed views.
- Keep selection, focus, enabled, playing and disabled states visually distinguishable.

---

## 8. Interaction state model

Define reusable state tokens/variants across components:

| State | Visual expectation |
|---|---|
| Default | Neutral presentation with clear affordance |
| Hover | Subtle background/border shift; does not move layout |
| Pressed | Slight darkening or elevation reduction |
| Focus-visible | Clear 2px-ish accessible outline, separated from component edge where possible |
| Active/open | Deep teal border or soft accent fill; not merely the hover effect |
| Selected | Persistent state with label/indicator and appropriate accessible semantics |
| Disabled | Reduced emphasis **while remaining legible**; no hover or interactive cursor |
| Loading | Stable button width; spinner/progress that doesn't cause layout shift |
| Error | Specific inline message; reserved error colour; clear recovery path |
| Success | Specific confirmation; does not rely solely on green colour |

### Operational state examples

- MIDI: disconnected / connecting / connected / unavailable or permission denied.
- Transport: ready / count-in / running / paused / completed.
- Score: current bar / upcoming bar / judged hit / missed note.
- Modal: closed / opening / open / closing.
- Exercise import: ready / selecting / importing / imported / failed.

Use a single source of truth for shared settings (BPM, count-in, subdivision, click timbre and metronome volume).

---

## 9. Design improvements recommended from screenshots

These are **proposals**, not claims about implementation bugs.

### P0 — resolve inconsistency before adding more UI

1. **Unify popovers:** extract a shared light-surface popover primitive; standardise corners, shadow, heading spacing, close icon, label/input style and dismissal. Preserve separate layouts for the two content types.
2. **Consolidate tokens:** use a fixed palette and spacing scale. Audit ad hoc teal values, varying card radii and mismatched control heights.
3. **Standardise form typography:** retain serif for editorial headings, sans for inputs and labels. The latter should not change face depending on which overlay hosts them.
4. **Fix responsive toolbar rules:** explicit icon layout and overflow design for small widths. No unpredictable wrapping or reshuffling.
5. **Review secondary-text contrast:** several descriptions and labels appear faint in dark panels; verify and adjust against AA targets.
6. **Align close interactions:** top-right X placement and Escape/outside-click behaviours should be consistent for each overlay type.

### P1 — improve score and workflow clarity

7. **Tune stave density:** four widely spaced staves make simple quarter-note exercises feel sparse. Introduce a musical-layout density policy rather than sizing staves simply to fill the container.
8. **Differentiate feedback vs control accent:** bright green score feedback and mint primary CTA are close; keep semantic distinctions clear via saturation/shape and documentation.
9. **Clarify range terminology:** “All bars”, From/To selection, loop count and Practice/Performance relationships need concise, unambiguous microcopy.
10. **Make the metronome strip more clearly actionable:** its tiny icon/status layout presently reads almost as a status-only indicator.
11. **Keep audio settings consistent across surfaces:** the metronome popover and right drawer should bind to the same underlying configuration, with matching field labels/options.
12. **Reduce competing emphasis:** primary CTA should remain dominant; close outlines, active tabs, badges and other mint elements should be less visually assertive.

### P2 — polish as the product grows

13. Add compact empty, loading and error states to the MIDI library.
14. Add tooltip and accessible labels to icon-only toolbar actions.
15. Define visual examples for disabled, paused, countdown and completed states.
16. Establish reusable chart styles for performance history and scoring using the same tokens without making the UI resemble a generic dashboard.
17. Add optional visual-density settings only if responsive score tuning alone proves insufficient.

---

## 10. Component architecture and implementation guidance

**Recommendations to Codex; adapt names to the existing codebase rather than introducing unnecessary duplication.**

Suggested conceptual component inventory:

```text
AppShell
├─ AppHeader
│  ├─ Brand
│  ├─ CurrentExerciseSummary
│  ├─ MidiConnectionStatus
│  └─ HeaderActions
├─ NotationCanvas
│  ├─ MetronomeTrigger
│  ├─ ScoreViewport
│  ├─ BarRangeTrigger
│  └─ ScoreProgress
└─ PerformanceActionBar
   ├─ RestartButton
   ├─ TransportButton
   └─ TimingLegend

Overlay primitives
├─ ModalFrame → MidiLibrary
├─ DrawerFrame → SoundAndDrumsSettings
└─ AnchoredPopover
   ├─ PracticeBarsPopover
   └─ MetronomePopover

Shared controls
├─ Button / IconButton
├─ SegmentedControl
├─ SelectField
├─ CheckboxField
├─ NumericStepper
├─ SliderField
├─ StatusBadge
└─ ExerciseCard
```

Implementation requirements:

1. **Audit before rewriting.** Inspect current CSS variables, global styles, component conventions and existing reusable controls.
2. **Retain working behaviours.** This is a style standardisation task, not permission to rewrite audio scheduling, MIDI recognition, hit scoring or notation engraving.
3. **Introduce semantic tokens.** Avoid direct hexadecimal colours or arbitrary pixel values within feature components.
4. **Create shared primitives only when useful.** Avoid heavyweight abstractions for one-off musical notation behaviour.
5. **Preserve a single state model.** Settings displayed in drawer and metronome popover must be genuinely shared.
6. **Use visual regression tests.** Capture the five source states at a consistent viewport, then test narrow tablet and mobile widths.
7. **Keep performance measurable.** Style changes must not create layout thrashing, audio glitches, dropped MIDI events or frame instability during long exercises.
8. **Do not introduce new fonts, frameworks or animation libraries** without demonstrating a clear improvement.

### Suggested token naming convention

Use domain-neutral primitives and semantic aliases:

```css
/* Illustrative proposals, not a drop-in replacement for existing styles. */
:root {
  --rd-color-app-bg: #0f1512;
  --rd-color-surface-dark: #151b18;
  --rd-color-score: #f4f0e6;
  --rd-color-surface-light: #f8f7f2;
  --rd-color-accent: #37d9c1;
  --rd-color-border-dark: #33433d;
  --rd-color-text-on-dark: #f0f3ee;
  --rd-color-text-on-light: #283a32;
  --rd-space-unit: 4px;
  --rd-radius-control: 12px;
  --rd-radius-popover: 16px;
  --rd-radius-panel: 22px;
}
```

Choose the **repository's existing naming style** if it already defines equivalent semantic tokens.

---

## 11. Acceptance criteria and QA checklist

### Fidelity

- [ ] Main view retains the dark shell, warm score, centred status/metronome and prominent mint action.
- [ ] Library modal retains dark two-column card layout at desktop with stable pagination.
- [ ] Drawer retains right-side dark presentation and clearly divided sections.
- [ ] Both popovers use the same shared light-surface styling while remaining anchored to their triggers.
- [ ] All controls use consistent typography, corner radii, border treatment and sizing.
- [ ] No ad hoc new accent colours are introduced.

### Behaviour

- [ ] Popovers close via X, Escape and outside click/tap.
- [ ] Popovers remain operable when pointer moves from trigger to control.
- [ ] Focus handling and dismissal return to a sensible control.
- [ ] Drawer and metronome popover reflect identical current settings.
- [ ] Setting changes respect safe playback/audio application semantics.
- [ ] Exercise-range selection validates bounds and updates displayed status.
- [ ] Overlay interactions do not unintentionally start, stop or reset an exercise.

### Responsiveness

- [ ] Header actions never overlap or reorder unpredictably.
- [ ] Library dialog and drawer remain navigable at narrow widths.
- [ ] Popovers stay within viewport bounds, including near screen edges.
- [ ] Main CTA and notation remain usable on tablet and mobile.
- [ ] No settings form generates horizontal scrolling.

### Accessibility and performance

- [ ] Colour contrast is measured against WCAG 2.2 AA targets.
- [ ] Keyboard navigation, focus visibility, Escape and control labels pass checks.
- [ ] User zoom at 200% and 400% preserves access to essential controls.
- [ ] Reduced-motion preferences are respected.
- [ ] Large/long exercises run without regressions to score responsiveness, MIDI reaction or audio scheduling.

---

## 12. Screenshot reference mapping

| Source image | Interface state | Evidence drawn from it |
|---|---|---|
| `Screenshot 2026-11-14 at 13.27.19.png` | MIDI library open | Modal surface, cards, tabs, import, pagination |
| `Screenshot 2026-11-14 at 13.27.25.png` | Sound & drums drawer | Settings layout, typography, dark controls |
| `Screenshot 2026-11-14 at 13.27.30.png` | Practice-bars popover | Light contextual panel, range selection |
| `Screenshot 2026-11-14 at 13.27.38.png` | Metronome popover | Compact light forms, two-column layout, anchoring |
| `Screenshot 2026-11-14 at 13.27.42.png` | Sound & drums drawer (second capture) | Repeat evidence for drawer consistency |

The screenshots were captured at **2047 × 1331 pixels** each, including browser and OS chrome. These are screenshot dimensions, **not** a declaration of the CSS viewport or app content breakpoints. Do not measure component CSS pixel sizes by directly equating screenshot pixels to CSS pixels without determining display scaling and viewport settings.

---

## 13. Next steps for a Codex implementation

**Phase 1 — Audit:** inventory existing design tokens, fonts, component styles and UI patterns; report differences against this guide. Identify exact current CSS values and any genuinely shared components. Do not make changes yet.

**Phase 2 — Standardise:** centralise colour, type, spacing, radius and control states; unify popovers; align settings forms, library styles and header actions, making only tightly scoped changes.

**Phase 3 — Responsive and accessibility:** implement deliberate tablet/mobile behaviour, focus management, contrast improvements, keyboard support, and accessibility checks.

**Phase 4 — Visual regression and performance:** compare reference views, test all relevant interaction states, and run existing MIDI/audio/long-score performance checks. Record deviations and explain deliberate choices.

**Non-goal:** An aesthetic overhaul. The intention is to preserve the current Rudiment visual identity and remove ambiguity, inconsistency and future style drift.

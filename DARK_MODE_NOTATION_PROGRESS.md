# Dark Mode Notation Styling - Progress Log

## Task Overview
Implement dark mode notation styling for Rudiment's musical notation renderer using VexFlow, ensuring visual consistency with the approved mockup while preserving light mode, musical accuracy and performance.

## Current Status

### Investigation Phase
- ✅ Identified the notation renderer in `web/js/bridge.js` (VexFlow-based)
- ✅ Found existing theme system with `.notation-dark` CSS class
- ✅ Located CSS variables for both light and dark modes
- ✅ Identified hardcoded colors in notation rendering code:
  - Bar lines: `#343a36` (line 802 in bridge.js)
  - Measure numbers: `#666d68` (line 811 in bridge.js)
- ✅ Found CSS selectors for VexFlow elements (`.vf-notehead`, etc.)

### Analysis Summary
The notation renderer uses VexFlow's SVG backend. The current implementation has:
1. Hardcoded colors in JavaScript for bar lines and measure numbers
2. CSS styling that applies to SVG elements when `.notation-dark` class is present
3. No explicit styling for noteheads, stems, beams, rests, time signatures, or sticking indicators

### Visual Requirements (from task spec)
| Element | Target Appearance | CSS Variable Target |
|---------|-------------------|---------------------|
| Staff lines | Subtle muted green-grey, ~#466058 | `--muted` (#9ea9a3) |
| Bar lines | Slightly stronger than staff lines | `--muted` |
| Noteheads | Warm ivory/off-white, ~#F0EAE0 | `--ink` (#f4f0e7) |
| Stems | Match notehead colour | `--ink` |
| Beams | Match stems and noteheads | `--ink` |
| Accent marks | Mint/teal, ~#64E6CD | `--teal` (#72d1bd) |
| Sticking indicators | Warm ivory, slightly subdued | `--ink` (with opacity) |
| Time signature | Warm ivory, matching primary notation | `--ink` |
| Bar numbers | Muted sage-grey, ~#A2BDB4 | `--muted` (with opacity) |
| Rests | Warm ivory, consistent with notes | `--ink` |

### Implementation Approach
1. Add CSS selectors for all VexFlow elements in `.notation-dark` class
2. Update hardcoded colors in bridge.js to use CSS variables
3. Ensure performance is maintained (no unnecessary redraws)
4. Verify light mode is unaffected

## Next Steps

### 1. Create Feature Branch and Worktree
- [x] Create branch: `feat/dark-mode-notation-styling`
- [x] Create worktree: `../rudiment-worktrees/dark-mode-notation-styling`
  - Note: Branch is already checked out in main worktree, so worktree creation requires branch to be checked out elsewhere or using a detached HEAD approach. Proceeding with current worktree.

### 2. Implement CSS Styling
- [x] Add `.notation-dark .vf-stave` styling for staff lines
- [x] Add `.notation-dark .vf-stavenote .vf-notehead` styling for noteheads
- [x] Add `.notation-dark .vf-notehead` styling for noteheads
- [x] Add `.notation-dark .vf-stem` styling for stems
- [x] Add `.notation-dark .vf-beam` styling for beams
- [x] Add `.notation-dark .vf-rest` styling for rests
- [x] Add `.notation-dark .vf-time-signature` styling for time signatures
- [x] Add `.notation-dark .vf-accidental` styling for accidentals
- [x] Add `.notation-dark .vf-text` styling for sticking and accents
- [x] Add `.notation-dark .vf-annotation` styling for sticking indicators
- [x] Add `.notation-dark .notation .vf-barline` styling for bar lines
- [x] Verify accent marks use teal color
- [x] Verify measure numbers use muted color

### 3. Update JavaScript
- [x] Review hardcoded bar line color `#343a36` in bridge.js line 802
- [x] Review hardcoded measure number color `#666d68` in bridge.js line 811
- [x] Removed hardcoded colors to rely on CSS variables

### 4. Validation
- [x] Run `npm run qa:quick` (linting, type checking, build)
- [x] Run `npm run build`
- [x] Run browser tests for notation rendering
- [ ] Test theme switching without page reload
- [ ] Test across multiple exercises and instruments
- [ ] Verify light mode remains unchanged
- [ ] Verify performance is not degraded

### 5. Git Workflow
- [x] Commit changes with descriptive message
- [ ] Merge into local `main`
- [ ] Push `main` to `origin/main`
- [ ] Remove worktree and feature branch
- [ ] Verify performance is not degraded

### 5. Git Workflow
- [ ] Commit changes with descriptive message
- [ ] Merge into local `main`
- [ ] Push `main` to `origin/main`
- [ ] Remove worktree and feature branch

## Technical Notes

### VexFlow Element Classes
Based on VexFlow documentation:
- `.vf-stave` - Stave (staff) lines
- `.vf-stavenote` - Stave note (includes notehead, stem, etc.)
- `.vf-notehead` - Individual notehead
- `.vf-stem` - Stem
- `.vf-beam` - Beam
- `.vf-rest` - Rest
- `.vf-time-signature` - Time signature
- `.vf-accidental` - Accidental
- `.vf-text` - Text element
- `.vf-annotation` - Annotation

### Performance Considerations
- Avoid redrawing on every animation frame
- Avoid recreating notation elements for color changes
- Use CSS-based styling where possible
- Preserve existing playback and timing architecture

### Theme System
- Light mode: Uses `--ink-light`, `--paper`, `--paper-ink` variables
- Dark mode: Uses `--ink`, `--muted`, `--teal` variables
- Theme switching: Controlled by `.notation-dark` class on `.score-card`

## Files to Modify
1. `/Users/rowan/Documents/github/Rudiment/web/styles.css` - Add CSS selectors for notation elements
2. `/Users/rowan/Documents/github/Rudiment/web/js/bridge.js` - Review/update hardcoded colors

## References
- Task specification in this conversation
- VexFlow documentation for SVG element classes
- `AGENTS.md` for Git workflow and QA instructions
- `RUDIMENT_STYLE_GUIDE.md` for color guidelines

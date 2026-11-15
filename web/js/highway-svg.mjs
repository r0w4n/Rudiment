// SVG presentation only: all positions and assessments come from the host.
const NS = 'http://www.w3.org/2000/svg';
const element = (name, attributes = {}) => {
  const node = document.createElementNS(NS, name);
  for (const [key, value] of Object.entries(attributes)) node.setAttribute(key, String(value));
  return node;
};

export function createHighwaySvg(viewport, record = null) {
  let svg, moving, grid, notes, playhead, width = 1, height = 1, scale = 1, model;
  const noteNodes = new Map(), gridNodes = new Map();

  function layout() {
    if (!svg) return;
    // Keep sixteenth notes legible on narrow screens by showing fewer beats.
    scale = Math.max(80, width / 10);
    svg.setAttribute('viewBox', `0 0 ${width} ${height}`);
    playhead.setAttribute('transform', `translate(${width / 2} 0)`);
    // Resize is rare; invalidate only the visible elements, not the clock.
    for (const node of noteNodes.values()) node.remove();
    for (const node of gridNodes.values()) node.remove();
    noteNodes.clear(); gridNodes.clear();
    if (model) draw(model);
  }

  function draw(presentation) {
    model = presentation;
    const start = record ? performance.now() : 0;
    const centre = height / 2;
    moving.setAttribute('transform', `translate(${width / 2 - presentation.position * scale} 0)`);
    const visibleGrid = new Set();
    for (const boundary of presentation.boundaries) {
      if (boundary.kind === 'subdivision') continue;
      const key = `${boundary.position}:${boundary.kind}`;
      visibleGrid.add(key);
      if (!gridNodes.has(key)) {
        const group = element('g', { transform: `translate(${boundary.position * scale} 0)`, class: `highway-grid-${boundary.kind}` });
        group.append(element('line', { x1: 0, x2: 0, y1: 26, y2: height - 20 }));
        if (boundary.kind === 'bar') {
          const label = element('text', { x: 7, y: 22 });
          label.textContent = `BAR ${boundary.bar}`;
          group.append(label);
        }
        grid.append(group); gridNodes.set(key, group);
      }
      if (boundary.kind === 'bar') {
        const label = gridNodes.get(key).querySelector('text');
        const text = `BAR ${boundary.bar}`;
        if (label.textContent !== text) label.textContent = text;
      }
    }
    for (const [key, node] of gridNodes) if (!visibleGrid.has(key)) { node.remove(); gridNodes.delete(key); }
    const visibleNotes = new Set();
    let previousGrace = null;
    let nextNode = notes.firstElementChild;
    for (const note of presentation.notes) {
      const gap = previousGrace ? note.position - previousGrace.position : Infinity;
      const graceRow = note.grace ? previousGrace && gap >= 0 && gap * scale < 20 ? previousGrace.row + 1 : 1 : 0;
      previousGrace = note.grace ? { position: note.position, row: graceRow } : null;
      const key = `${note.iteration}:${note.id}`;
      visibleNotes.add(key);
      let group = noteNodes.get(key);
      const signature = `${note.hand ?? ''}:${note.accent}:${note.grace}`;
      if (group && group.getAttribute('data-signature') !== signature) {
        if (group === nextNode) nextNode = group.nextElementSibling;
        group.remove(); noteNodes.delete(key); group = null;
      }
      if (!group) {
        group = element('g', { class: 'highway-note', role: 'img', 'data-note-id': note.id, 'data-iteration': note.iteration, 'data-signature': signature, 'data-grace': note.grace });
        group.append(element('rect', { x: -9, y: -11, width: 18, height: 22, rx: 5 }));
        group.append(element('text', { class: 'highway-judgement', x: 0, y: 4, 'text-anchor': 'middle', 'aria-hidden': 'true' }));
        if (note.accent) {
          const accent = element('text', { class: 'highway-accent', x: 0, y: note.grace ? -30 : -20, 'text-anchor': 'middle' });
          accent.textContent = '>';
          group.append(accent);
        }
        if (note.hand === 'R' || note.hand === 'L') {
          const hand = element('text', { class: 'highway-hand', x: 0, y: note.grace ? -18 : 32, 'text-anchor': 'middle' });
          hand.textContent = note.hand;
          group.append(hand);
        }
        const title = element('title'); group.append(title);
        notes.append(group); noteNodes.set(key, group);
      }
      // Keep accessible reading order musical, moving nodes only when needed.
      if (group !== nextNode) notes.insertBefore(group, nextNode);
      nextNode = group.nextElementSibling;
      if (group.getAttribute('data-position') !== String(note.position) || group.getAttribute('data-grace-row') !== String(graceRow)) {
        group.setAttribute('data-position', String(note.position));
        group.setAttribute('data-grace-row', String(graceRow));
        group.setAttribute('transform', `translate(${note.position * scale} ${centre - graceRow * 34})`);
      }
      const judgement = note.judgement || 'expected';
      if (group.getAttribute('data-judgement') !== judgement) {
        group.setAttribute('data-judgement', judgement);
        group.querySelector('.highway-judgement').textContent = ({ perfect: 'P', good: 'G', early: 'E', late: 'L', missed: '×' })[judgement] ?? '';
        group.lastChild.textContent = `${note.instrument}${note.grace ? ' grace' : ''} · ${judgement}${note.hand ? ` · ${note.hand}` : ''}`;
        group.setAttribute('aria-label', group.lastChild.textContent);
      }
    }
    for (const [key, node] of noteNodes) if (!visibleNotes.has(key)) { node.remove(); noteNodes.delete(key); }
    if (record) record(performance.now() - start);
  }

  return {
    initialize() {
      svg = element('svg', { class: 'highway-svg', role: 'group', 'aria-label': 'Snare timeline. Notes move toward the centred playhead.', preserveAspectRatio: 'none' });
      const lane = element('rect', { class: 'highway-lane', x: 0, y: '30%', width: '100%', height: '40%' });
      const centreline = element('line', { class: 'highway-centreline', x1: 0, x2: '100%', y1: '50%', y2: '50%' });
      moving = element('g', { class: 'highway-moving' });
      grid = element('g'); notes = element('g'); moving.append(grid, notes);
      playhead = element('g', { class: 'highway-playhead' });
      playhead.append(element('line', { x1: 0, x2: 0, y1: 15, y2: '100%' }), element('path', { d: 'M -6 9 L 6 9 L 0 16 Z' }));
      svg.append(lane, centreline, moving, playhead); viewport.append(svg);
    },
    update: draw,
    resize(size) { width = Math.max(1, size.width); height = Math.max(1, size.height); layout(); },
    dispose() { svg?.remove(); noteNodes.clear(); gridNodes.clear(); model = null; svg = null; },
  };
}

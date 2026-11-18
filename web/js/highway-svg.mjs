// SVG presentation only: all positions and assessments come from the host.
const NS = 'http://www.w3.org/2000/svg';
const element = (name, attributes = {}) => {
  const node = document.createElementNS(NS, name);
  for (const [key, value] of Object.entries(attributes)) node.setAttribute(key, String(value));
  return node;
};
export function createHighwaySvg(viewport, record = null) {
  let svg, laneLayer, moving, grid, notes, strikes, playhead, width = 1, height = 1, scale = 1, model, laneSignature = '';
  const noteNodes = new Map(), strikeNodes = new Map(), gridNodes = new Map();

  function laneCentre(lane, presentation) {
    const index = Math.max(0, presentation.lanes.findIndex(item => item.instrument === lane));
    return (index + .5) * height / Math.max(1, presentation.lanes.length);
  }

  function drawLanes(presentation) {
    const signature = `${width}:${height}:${presentation.lanes.map(lane => `${lane.instrument}:${lane.label}`).join('|')}`;
    if (signature === laneSignature) return;
    laneSignature = signature;
    laneLayer.replaceChildren();
    const laneHeight = height / Math.max(1, presentation.lanes.length);
    presentation.lanes.forEach((lane, index) => {
      const y = index * laneHeight;
      laneLayer.append(element('rect', { class: 'highway-lane', x: 0, y: y + 2, width: '100%', height: Math.max(1, laneHeight - 4), 'data-lane': lane.instrument }));
      laneLayer.append(element('line', { class: 'highway-centreline', x1: 0, x2: '100%', y1: y + laneHeight / 2, y2: y + laneHeight / 2 }));
      const label = element('text', { class: 'highway-lane-name', x: 9, y: y + 16 });
      label.textContent = lane.label;
      laneLayer.append(label);
    });
  }

  function layout() {
    if (!svg) return;
    // Keep sixteenth notes legible on narrow screens by showing fewer beats.
    scale = Math.max(80, width / 10);
    svg.setAttribute('viewBox', `0 0 ${width} ${height}`);
    playhead.setAttribute('transform', `translate(${width / 2} 0)`);
    // Resize is rare; invalidate only the visible elements, not the clock.
    for (const node of noteNodes.values()) node.remove();
    for (const node of strikeNodes.values()) node.remove();
    for (const node of gridNodes.values()) node.remove();
    noteNodes.clear(); strikeNodes.clear(); gridNodes.clear();
    if (model) draw(model);
  }

  function draw(presentation) {
    model = presentation;
    const start = record ? performance.now() : 0;
    drawLanes(presentation);
    moving.setAttribute('transform', `translate(${width / 2 - presentation.position * scale} 0)`);
    const visibleGrid = new Set();
    for (const boundary of presentation.boundaries) {
      if (boundary.kind === 'subdivision') continue;
      const key = `${boundary.position}:${boundary.kind}`;
      visibleGrid.add(key);
      if (!gridNodes.has(key)) {
        const group = element('g', { transform: `translate(${boundary.position * scale} 0)`, class: `highway-grid-${boundary.kind}` });
        group.append(element('line', { x1: 0, x2: 0, y1: 8, y2: height - 8 }));
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
      const signature = `${note.hand ?? ''}:${note.accent}:${note.grace}:${note.instrument}:${note.articulation}:${note.lane}`;
      if (group && group.getAttribute('data-signature') !== signature) {
        if (group === nextNode) nextNode = group.nextElementSibling;
        group.remove(); noteNodes.delete(key); group = null;
      }
      if (!group) {
        group = element('g', { class: 'highway-note', role: 'img', 'data-note-id': note.id, 'data-iteration': note.iteration, 'data-signature': signature, 'data-grace': note.grace, 'data-instrument': note.instrument, 'data-lane': note.lane, 'data-articulation': note.articulation });
        group.append(element('rect', { x: -9, y: -11, width: 18, height: 22, rx: 5 }));
        if (note.instrument === 'closed_hat' || note.instrument === 'pedal_hat' || note.instrument === 'open_hat') {
          const articulation = element('text', { class: 'highway-hat-articulation', x: 0, y: 4, 'text-anchor': 'middle' });
          articulation.textContent = note.instrument === 'open_hat' ? 'O' : 'X';
          group.append(articulation);
        }
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
        const laneY = laneCentre(note.lane, presentation);
        const graceOffset = Math.min(28, Math.max(24, height / Math.max(1, presentation.lanes.length) * .24));
        group.setAttribute('transform', `translate(${note.position * scale} ${laneY - graceRow * graceOffset})`);
      }
      const judgement = note.judgement || 'expected';
      if (group.getAttribute('data-judgement') !== judgement) {
        group.setAttribute('data-judgement', judgement);
        group.lastChild.textContent = `${note.instrument}${note.grace ? ' grace' : ''} · ${judgement}${note.hand ? ` · ${note.hand}` : ''}`;
        group.setAttribute('aria-label', group.lastChild.textContent);
      }
    }
    for (const [key, node] of noteNodes) if (!visibleNotes.has(key)) { node.remove(); noteNodes.delete(key); }
    const visibleStrikes = new Set();
    for (const hit of presentation.unmatched ?? []) {
      const key = String(hit.id);
      visibleStrikes.add(key);
      let marker = strikeNodes.get(key);
      if (!marker) {
        marker = element('circle', { class: 'highway-unmatched-hit', role: 'img', r: 4, 'data-event-id': key });
        strikes.append(marker); strikeNodes.set(key, marker);
      }
      marker.setAttribute('cx', String(hit.position * scale));
      marker.setAttribute('cy', String(laneCentre(hit.lane, presentation)));
      marker.setAttribute('data-position', String(hit.position));
      marker.setAttribute('data-iteration', String(hit.iteration));
      marker.setAttribute('data-instrument', hit.instrument);
      marker.setAttribute('data-judgement', hit.judgement);
      marker.setAttribute('aria-label', `${hit.instrument} unmatched strike · ${hit.judgement.replace('_', ' ')}`);
    }
    for (const [key, node] of strikeNodes) if (!visibleStrikes.has(key)) { node.remove(); strikeNodes.delete(key); }
    if (record) record(performance.now() - start);
  }

  return {
    initialize() {
      svg = element('svg', { class: 'highway-svg', role: 'group', 'aria-label': 'Drum timeline. Notes move toward the centred playhead.', preserveAspectRatio: 'none' });
      laneLayer = element('g', { class: 'highway-lanes' });
      moving = element('g', { class: 'highway-moving' });
      grid = element('g'); notes = element('g'); strikes = element('g', { class: 'highway-unmatched-hits' }); moving.append(grid, notes, strikes);
      playhead = element('g', { class: 'highway-playhead' });
      playhead.append(element('line', { x1: 0, x2: 0, y1: 15, y2: '100%' }), element('path', { d: 'M -6 9 L 6 9 L 0 16 Z' }));
      svg.append(laneLayer, moving, playhead); viewport.append(svg);
    },
    update: draw,
    resize(size) { width = Math.max(1, size.width); height = Math.max(1, size.height); layout(); },
    dispose() { svg?.remove(); noteNodes.clear(); strikeNodes.clear(); gridNodes.clear(); model = null; laneSignature = ''; svg = null; },
  };
}

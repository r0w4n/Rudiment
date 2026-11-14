import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';

const wcagTags = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'];

function contrastRatio(first: string, second: string): number {
  const luminance = (color: string) => {
    const components = color.match(/\d+/g)?.slice(0, 3).map(Number);
    if (!components || components.length !== 3) throw new Error(`Unexpected color: ${color}`);
    return components
      .map((component) => {
        const value = component / 255;
        return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
      })
      .reduce((sum, component, index) => sum + component * [0.2126, 0.7152, 0.0722][index], 0);
  };
  const [lighter, darker] = [luminance(first), luminance(second)].sort((a, b) => b - a);
  return (lighter + 0.05) / (darker + 0.05);
}

async function expectNoWcagViolations(page: Page, context: string, disabledRules: string[] = []) {
  const results = await new AxeBuilder({ page }).withTags(wcagTags).disableRules(disabledRules).analyze();
  expect(
    results.violations.map((violation) => ({
      id: violation.id,
      impact: violation.impact,
      help: violation.help,
      nodes: violation.nodes.map((node) => ({
        target: node.target.join(' '),
        html: node.html,
        failureSummary: node.failureSummary,
      })),
    })),
    `${context} has automated WCAG violations`,
  ).toEqual([]);
}

test('loop control names include the selected visible option', async ({ page }) => {
  await page.goto('/');
  const loops = page.getByRole('combobox', { name: /Loops/ });
  for (const [value, text] of [
    ['1', '1 / 1 LOOP'],
    ['2', '2 LOOPS'],
    ['4', '4 LOOPS'],
    ['0', '∞ LOOP'],
  ]) {
    await loops.selectOption(value);
    await expect(loops).toHaveAccessibleName(`Loops: ${text}`);
    await expect(loops.locator('option:checked')).toHaveText(text);
  }
});

test('paper controls retain a visible keyboard focus outline', async ({ page }) => {
  await page.goto('/');
  await page.keyboard.press('Tab');
  const expectFocusOutline = async (selector: string) => {
    const control = page.locator(selector);
    await control.focus();
    await expect(control).toBeFocused();
    const style = await control.evaluate((element) => {
      const computed = getComputedStyle(element);
      return {
        style: computed.outlineStyle,
        width: Number.parseFloat(computed.outlineWidth),
        color: computed.outlineColor,
      };
    });
    expect(style.style).not.toBe('none');
    expect(style.width).toBeGreaterThanOrEqual(2);
    expect(style.color).toBe('rgb(36, 108, 91)');
  };
  await expectFocusOutline('.loop-control');
  await page.locator('#bar-selection-trigger').click();
  await page.keyboard.press('Tab');
  await expectFocusOutline('#bar-selection-popover .bar-selection-close');
  await page.keyboard.press('Escape');
  await page.locator('#visual-metronome').click();
  await page.keyboard.press('Tab');
  await expectFocusOutline('#metronome-popover .bar-selection-close');
  const popup = page.locator('#metronome-popover');
  await popup.getByLabel('Tempo in BPM').focus();
  await expect(popup.locator('.tempo-control')).not.toHaveCSS('box-shadow', 'none');
});

test('light popover fields use a light native scheme and contrast-safe colors', async ({ page }) => {
  await page.goto('/');
  await page.locator('#visual-metronome').click();
  const popup = page.locator('#metronome-popover');
  const fields = popup.locator('select');
  await expect(fields).toHaveCount(3);
  for (const field of await fields.all()) {
    const style = await field.evaluate((element) => {
      const computed = getComputedStyle(element);
      return {
        scheme: computed.colorScheme,
        foreground: computed.color,
        background: computed.backgroundColor,
        border: computed.borderTopColor,
      };
    });
    expect(style.scheme).toBe('light');
    expect(contrastRatio(style.foreground, style.background)).toBeGreaterThanOrEqual(4.5);
    expect(contrastRatio(style.border, style.background)).toBeGreaterThanOrEqual(3);
    expect(contrastRatio(style.border, 'rgb(248, 247, 242)')).toBeGreaterThanOrEqual(3);
  }
  const panelColors = await popup.locator('.metronome-count-in').evaluate((element) => ({
    foreground: getComputedStyle(element).color,
    background: getComputedStyle(element.closest('.bar-selection-popover')!).backgroundColor,
  }));
  expect(contrastRatio(panelColors.foreground, panelColors.background)).toBeGreaterThanOrEqual(4.5);
  expect(contrastRatio('rgb(36, 108, 91)', panelColors.background)).toBeGreaterThanOrEqual(3);
  const previewStyle = await popup.getByRole('button', { name: 'Preview' }).evaluate((element) => {
    const style = getComputedStyle(element);
    return {
      font: style.fontFamily,
      foreground: style.color,
      background: style.backgroundColor,
      border: style.borderTopColor,
    };
  });
  expect(previewStyle.font).toContain('system-ui');
  expect(contrastRatio(previewStyle.foreground, previewStyle.background)).toBeGreaterThanOrEqual(4.5);
  expect(contrastRatio(previewStyle.border, panelColors.background)).toBeGreaterThanOrEqual(3);
  await page.getByLabel('Select bars').click();
  await expect(page.locator('#bar-selection-popover select')).toHaveCount(2);
  await expect(page.locator('#bar-selection-popover select').first()).toHaveCSS('color-scheme', 'light');
});

test('result chart judgement colors contrast with the track', async ({ page }) => {
  await page.goto('/');
  const colors = await page.evaluate(() => {
    return ['perfect', 'good', 'early', 'late', 'missed'].map((judgement) => {
      const row = document.createElement('div');
      row.className = `breakdown-row ${judgement}`;
      const track = document.createElement('em');
      const fill = document.createElement('i');
      track.append(fill);
      row.append(track);
      document.body.append(row);
      const result = { fill: getComputedStyle(fill).backgroundColor, track: getComputedStyle(track).backgroundColor };
      row.remove();
      return result;
    });
  });
  for (const { fill, track } of colors) expect(contrastRatio(fill, track)).toBeGreaterThanOrEqual(3);
});

test('settings drawer remains scrollable and closable at narrow and zoomed sizes', async ({ page }) => {
  // 320x180 approximates the CSS viewport at 400% browser zoom on 1280x720.
  // CSS zoom itself does not resize viewport units like browser zoom does.
  for (const { width, height } of [
    { width: 320, height: 568 },
    { width: 844, height: 390 },
    { width: 320, height: 180 },
  ]) {
    await page.setViewportSize({ width, height });
    await page.goto('/');
    await page.getByRole('button', { name: 'Open settings' }).click();
    const drawer = page.getByRole('dialog', { name: 'Sound & drums' });
    const reset = drawer.getByRole('button', { name: 'Reset settings & mappings' });
    await reset.scrollIntoViewIfNeeded();
    await expect(reset).toBeInViewport();
    await expect(drawer.getByRole('button', { name: 'Close settings' })).toBeInViewport();
    await page.keyboard.press('Escape');
    await expect(drawer).toBeHidden();
  }
});

test('library controls remain reachable in a very short viewport', async ({ page }) => {
  await page.setViewportSize({ width: 320, height: 180 });
  await page.goto('/');
  await page.getByRole('button', { name: 'MIDI library' }).click();
  const library = page.getByRole('dialog', { name: 'MIDI library' });
  const card = library.locator('.library-load').first();
  await card.scrollIntoViewIfNeeded();
  await expect(card).toBeInViewport();
  const next = library.getByRole('button', { name: 'Next library page' });
  await next.scrollIntoViewIfNeeded();
  await next.click();
  await expect(library.getByLabel('Library pagination')).toContainText('Page 2');
  const close = library.getByRole('button', { name: 'Close MIDI library' });
  await close.scrollIntoViewIfNeeded();
  await expect(close).toBeInViewport();
  await close.click();
  await expect(library).toBeHidden();
});

test('practice workspace and score feedback pass automated WCAG checks', async ({ page }) => {
  await page.goto('/');
  await expect(page.locator('.score-note').first()).toBeVisible();
  await expectNoWcagViolations(page, 'Practice workspace');

  await page.evaluate(() => {
    const app = (window as any).rudiment;
    app.updateScore(0, [
      { expected_beat: 0, actual_beat: 0, offset_ms: 0, judgement: 'perfect', score: 100 },
      { expected_beat: 0.5, actual_beat: 0.45, offset_ms: -50, judgement: 'early', score: 70 },
    ]);
  });
  await expect(page.locator('.score-note[data-beat="0"] .vf-notehead')).toHaveAttribute('aria-label', /perfect/);
  await expect(page.locator('.score-note[data-beat="0.5"] .vf-notehead')).toHaveAttribute('aria-label', /early/);
  await expectNoWcagViolations(page, 'Text-equivalent score feedback');
});

test('library, settings, and statistics dialogs pass axe and manage focus', async ({ page }) => {
  await page.goto('/');
  await expect(page.locator('.score-note').first()).toBeVisible();

  for (const dialog of [
    { open: 'MIDI library', name: 'MIDI library' },
    { open: 'Open settings', name: 'Sound & drums' },
    { open: 'Stats', name: 'Performance history' },
  ]) {
    const trigger = page.getByRole('button', { name: dialog.open });
    await trigger.click();
    const modal = page.getByRole('dialog', { name: dialog.name });
    await expect(modal).toBeVisible();
    await expect.poll(() => modal.evaluate((element) => element.contains(document.activeElement))).toBe(true);
    await expectNoWcagViolations(page, `${dialog.name} dialog`);
    await page.keyboard.press('Escape');
    await expect(modal).toBeHidden();
    await expect(trigger).toBeFocused();
  }
});

test('contextual metronome and bar controls pass axe and restore focus', async ({ page }) => {
  await page.goto('/');
  for (const [triggerSelector, dialogName] of [
    ['#visual-metronome', 'Metronome'],
    ['#bar-selection-trigger', 'Bar selection'],
  ]) {
    const trigger = page.locator(triggerSelector);
    await trigger.focus();
    await page.keyboard.press('Space');
    await expect(trigger).toHaveAttribute('aria-expanded', 'true');
    await expect(page.getByRole('dialog', { name: dialogName })).toBeVisible();
    if (dialogName === 'Metronome') {
      const dialog = page.getByRole('dialog', { name: dialogName });
      await dialog.getByLabel('Increase tempo').focus();
      await page.keyboard.press('Space');
      await expect(dialog.getByLabel('Tempo in BPM')).toHaveValue('61');
      await expect(page.getByRole('button', { name: /Start practice/ })).toBeVisible();
    }
    await expectNoWcagViolations(page, `${dialogName} popover`);
    await page.keyboard.press('Escape');
    await expect(trigger).toBeFocused();
    await expect(trigger).toHaveAttribute('aria-expanded', 'false');
  }
});

test('keyboard focus remains visible at narrow and zoomed layouts', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/');
  await expect(page.locator('.score-note').first()).toBeVisible();
  await expectNoWcagViolations(page, 'Narrow mobile layout');

  await page.setViewportSize({ width: 1280, height: 720 });
  await page.evaluate(() => {
    document.documentElement.style.zoom = '200%';
  });
  await page.keyboard.press('Tab');
  const focused = page.locator(':focus');
  await expect(focused).toBeVisible();
  const focusStyle = await focused.evaluate((element) => {
    const style = getComputedStyle(element);
    return { outlineStyle: style.outlineStyle, outlineWidth: style.outlineWidth };
  });
  expect(focusStyle.outlineStyle).not.toBe('none');
  expect(Number.parseFloat(focusStyle.outlineWidth)).toBeGreaterThanOrEqual(2);
  // CSS zoom is the closest deterministic Playwright approximation to browser
  // zoom. Axe's target-size geometry double-applies that transform, so target
  // size is enforced by the unzoomed desktop and narrow scans above.
  await expectNoWcagViolations(page, 'Desktop 200% zoom layout', ['target-size']);
});

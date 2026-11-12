import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';

const wcagTags = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'];

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

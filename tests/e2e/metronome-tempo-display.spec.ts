import { test, expect, selectDisplayMode } from './fixtures';

// Issue #18: the BPM value shown above the metronome tempo slider must follow
// the thumb live, wherever the main-app metronome popover appears, and always
// match the actual selected tempo after reopening the popover.

async function openMetronomePopover(page: import('@playwright/test').Page) {
  await page.locator('#visual-metronome').click();
  await expect(page.getByRole('dialog', { name: 'Metronome' })).toBeVisible();
  const popover = page.locator('#metronome-popover');
  return {
    popover,
    slider: popover.getByLabel('Tempo slider'),
    bpmInput: popover.getByLabel('Tempo in BPM'),
  };
}

async function dragSlider(page: import('@playwright/test').Page, slider: import('@playwright/test').Locator, bpmInput: import('@playwright/test').Locator, fromRatio: number, toRatio: number) {
  const box = await slider.boundingBox();
  if (!box) throw new Error('Tempo slider has no bounding box');
  const y = box.y + box.height / 2;
  await page.mouse.move(box.x + box.width * fromRatio, y);
  await page.mouse.down();
  const samples: Array<[string, string]> = [];
  const steps = 6;
  for (let step = 1; step <= steps; step++) {
    const ratio = fromRatio + ((toRatio - fromRatio) * step) / steps;
    await page.mouse.move(box.x + box.width * ratio, y, { steps: 2 });
    await page.waitForTimeout(50);
    samples.push([await slider.inputValue(), await bpmInput.inputValue()]);
  }
  await page.mouse.up();
  return samples;
}

test('dragging the tempo slider updates the BPM display live', async ({ page }) => {
  await page.goto('/');
  const { slider, bpmInput } = await openMetronomePopover(page);
  await expect(bpmInput).toHaveValue('60');
  const samples = await dragSlider(page, slider, bpmInput, 0.2, 0.85);
  // The BPM display must track the slider on every intermediate input event,
  // not only after the drag commits or the popover is reopened.
  for (const [sliderValue, displayValue] of samples) {
    const sliderBpm = Number(sliderValue);
    expect(sliderBpm).toBeGreaterThan(30);
    expect(sliderBpm).toBeLessThanOrEqual(240);
    expect(displayValue, `display ${displayValue} vs slider ${sliderValue}`).toBe(sliderValue);
  }
  await expect(bpmInput).toHaveValue(await slider.inputValue());
});

test('keyboard adjustment updates the BPM display immediately', async ({ page }) => {
  await page.goto('/');
  const { slider, bpmInput } = await openMetronomePopover(page);
  await slider.focus();
  for (let step = 0; step < 3; step++) {
    await page.keyboard.press('ArrowRight');
    await expect(bpmInput).toHaveValue(await slider.inputValue());
  }
  await expect(slider).toHaveValue('63');
  for (let step = 0; step < 2; step++) {
    await page.keyboard.press('ArrowLeft');
    await expect(bpmInput).toHaveValue(await slider.inputValue());
  }
  await expect(slider).toHaveValue('61');
});

test('displayed BPM matches the slider across the allowed range', async ({ page }) => {
  await page.goto('/');
  const { slider, bpmInput } = await openMetronomePopover(page);
  await slider.focus();
  await page.keyboard.press('Home');
  await expect(slider).toHaveValue('30');
  await expect(bpmInput).toHaveValue('30');
  await page.keyboard.press('End');
  await expect(slider).toHaveValue('240');
  await expect(bpmInput).toHaveValue('240');
});

test('selected tempo stays applied after reopening the popover', async ({ page }) => {
  await page.goto('/');
  let controls = await openMetronomePopover(page);
  const samples = await dragSlider(page, controls.slider, controls.bpmInput, 0.5, 0.4);
  const finalBpm = samples[samples.length - 1][0];
  await page.keyboard.press('Escape');
  await expect(page.getByRole('dialog', { name: 'Metronome' })).toBeHidden();
  await expect(page.locator('#visual-metronome')).toHaveAttribute('aria-label', new RegExp(`${finalBpm} BPM`));

  controls = await openMetronomePopover(page);
  await expect(controls.slider).toHaveValue(finalBpm);
  await expect(controls.bpmInput).toHaveValue(finalBpm);
});

test('Highway view popover keeps the BPM display in sync while dragging', async ({ page }) => {
  await page.goto('/');
  await selectDisplayMode(page, 'Highway');
  const { slider, bpmInput } = await openMetronomePopover(page);
  const samples = await dragSlider(page, slider, bpmInput, 0.2, 0.7);
  for (const [sliderValue, displayValue] of samples) {
    expect(displayValue, `display ${displayValue} vs slider ${sliderValue}`).toBe(sliderValue);
  }
  await expect(bpmInput).toHaveValue(await slider.inputValue());
});

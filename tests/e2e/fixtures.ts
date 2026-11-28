import { expect as baseExpect, test as baseTest, type Page } from '@playwright/test';

// Since issue #34 the Highway/Notation selector lives in the Settings drawer,
// so selecting a display mode opens the drawer, clicks the mode and closes it.
export async function selectDisplayMode(page: Page, mode: 'Highway' | 'Notation') {
  await page.getByRole('button', { name: 'Open settings' }).click();
  await page.locator('#settings-dialog').getByRole('button', { name: mode, exact: true }).click();
  await page.getByRole('button', { name: 'Close settings' }).first().click();
}

// Most specs exercise practice flows on an opted-out profile so the first-run
// onboarding page does not cover the workspace. The dedicated onboarding spec
// covers the fresh-profile flow itself.
export const test = baseTest.extend({
  page: async ({ page }, use) => {
    await page.addInitScript(() => {
      localStorage.setItem('rudiment-onboarding-skipped', 'true');
    });
    await use(page);
  },
});

export { baseExpect as expect };

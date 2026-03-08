import { expect, test } from '@playwright/test';

test('attacker king in source data does not block drops on visible empty squares', async ({ page }) => {
  await page.route(/\/puzzles\/1\.json/, async (route) => {
    const puzzles = [
      {
        id: 52,
        mateLength: 1,
        initial: {
          pieces: [
            { x: 5, y: 1, owner: 'defender', type: 'K' },
            { x: 5, y: 3, owner: 'attacker', type: 'P' },
            { x: 5, y: 5, owner: 'attacker', type: 'K' },
          ],
          hands: {
            attacker: { R: 0, B: 0, G: 1, S: 0, N: 0, L: 0, P: 0 },
            defender: { R: 0, B: 0, G: 0, S: 0, N: 0, L: 0, P: 0 },
          },
          sideToMove: 'attacker',
        },
        solution: [{ drop: 'G', to: [5, 2], promote: false }],
        quality: 'validated',
        score: 9,
      },
    ];
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(puzzles),
    });
  });

  await page.goto('/?mate=1&id=52');
  await expect(page.getByRole('heading', { name: '1手詰 #52' })).toBeVisible();

  await page.getByRole('button', { name: '金' }).click();

  const target = page.locator("button[data-x='5'][data-y='5']");
  await expect(target).toHaveClass(/move-target/);
  await target.click();
  await expect(target).toHaveClass(/last-move/);
});

test('1手詰 #54 allows dropping Gold on correct square', async ({ page }) => {
  await page.goto('/?mate=1&id=54');
  await expect(page.getByRole('heading', { name: '1手詰 #54' })).toBeVisible();

  // 持ち駒の金を選択（駒台内のボタン）
  await page.locator('.komadai-piece', { hasText: '金' }).click();
  // 正解手: (5,2)に金打ち
  const target = page.locator("button[data-x='5'][data-y='2']");
  await expect(target).toHaveClass(/move-target/);
  await target.click();
  await expect(target).toHaveClass(/last-move/);
});

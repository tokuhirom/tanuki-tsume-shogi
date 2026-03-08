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

test('1手詰 allows dropping Gold on correct square (hash-based)', async ({ page }) => {
  // 持ち駒に金がある1手詰を動的に取得
  const puzzlesRes = await (await fetch('http://localhost:4173/puzzles/1.json')).json().catch(() => null);
  const puzzle = puzzlesRes?.find(p => p.initial.hands.attacker.G > 0 && p.solution[0]?.drop === 'G');
  test.skip(!puzzle, 'No gold drop puzzle found');

  await page.goto(`/?mate=1&pid=${puzzle.hash}`);
  await expect(page.locator('h2')).toContainText('1手詰');

  // 持ち駒の金を選択（駒台内のボタン）
  await page.locator('.komadai-piece', { hasText: '金' }).click();
  // 正解手の位置に打つ
  const target = page.locator(`button[data-x='${puzzle.solution[0].to[0]}'][data-y='${puzzle.solution[0].to[1]}']`);
  await expect(target).toHaveClass(/move-target/);
  await target.click();
  await expect(target).toHaveClass(/last-move/);
});

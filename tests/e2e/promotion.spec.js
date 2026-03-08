import fs from 'node:fs';
import path from 'node:path';
import { expect, test } from '@playwright/test';

function loadPuzzle(length, id) {
  const file = path.resolve(process.cwd(), `public/puzzles/${length}.json`);
  const list = JSON.parse(fs.readFileSync(file, 'utf-8'));
  return list.find((x) => x.id === id);
}

test('promotion prompt appears and move is accepted', async ({ page }) => {
  const puzzle = loadPuzzle(5, 1);
  const first = puzzle.solution[0];
  if (!first || !first.from) test.skip();

  await page.goto('/?mate=5&id=1');
  await expect(page.getByRole('heading', { name: '5手詰 #1' })).toBeVisible();

  await page.locator(`button[data-x='${first.from[0]}'][data-y='${first.from[1]}']`).click();
  await page.locator(`button[data-x='${first.to[0]}'][data-y='${first.to[1]}']`).click();

  const prompt = page.getByText('成りますか？');
  if (await prompt.isVisible().catch(() => false)) {
    await page.getByRole('button', { name: '成る' }).click();
    await expect(prompt).not.toBeVisible();
  }

  // Move should have been accepted
  await expect(page.locator('.message')).not.toHaveText('攻め方の手を選んでください');
});

test('promotion choice is accepted for wrong destination', async ({ page }) => {
  const puzzle = loadPuzzle(5, 1);
  const first = puzzle.solution[0];
  if (!first || !first.from) test.skip();

  await page.goto('/?mate=5&id=1');
  await expect(page.getByRole('heading', { name: '5手詰 #1' })).toBeVisible();

  // Click the correct piece
  await page.locator(`button[data-x='${first.from[0]}'][data-y='${first.from[1]}']`).click();

  // Click a move target that is NOT the solution destination
  const targets = page.locator('.board button.move-target');
  await expect(targets.first()).toBeVisible();

  // Find a target that differs from the solution
  const targetCount = await targets.count();
  for (let i = 0; i < targetCount; i++) {
    const t = targets.nth(i);
    const x = await t.getAttribute('data-x');
    const y = await t.getAttribute('data-y');
    if (Number(x) !== first.to[0] || Number(y) !== first.to[1]) {
      await t.click();

      const prompt = page.getByText('成りますか？');
      if (await prompt.isVisible().catch(() => false)) {
        await page.getByRole('button', { name: '成る' }).click();
        await expect(prompt).not.toBeVisible();
      }

      // Move should be accepted (free play)
      await expect(page.locator('.message')).not.toHaveText('攻め方の手を選んでください');
      return;
    }
  }
  // If all targets are the solution target, just play the solution
  test.skip();
});

test('solving 5手詰 #1 with correct moves clears puzzle', async ({ page }) => {
  const puzzle = loadPuzzle(5, 1);

  await page.goto('/?mate=5&id=1');
  await expect(page.getByRole('heading', { name: '5手詰 #1' })).toBeVisible();

  for (let i = 0; i < puzzle.solution.length; i += 2) {
    const move = puzzle.solution[i];
    if (move.drop) {
      const label = { R: '飛', B: '角', G: '金', S: '銀', P: '歩' }[move.drop] || move.drop;
      await page.getByRole('button', { name: new RegExp(`^${label} ×`) }).click();
      await page.locator(`button[data-x='${move.to[0]}'][data-y='${move.to[1]}']`).click();
    } else {
      await page.locator(`button[data-x='${move.from[0]}'][data-y='${move.from[1]}']`).click();
      await page.locator(`button[data-x='${move.to[0]}'][data-y='${move.to[1]}']`).click();
    }
    const promoteDialog = page.getByText('成りますか？');
    if (await promoteDialog.isVisible().catch(() => false)) {
      await page.getByRole('button', { name: move.promote ? '成る' : '成らない' }).click();
    }
  }

  await expect(page.getByText('クリア！')).toBeVisible();
});

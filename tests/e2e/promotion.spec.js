import fs from 'node:fs';
import path from 'node:path';
import { expect, test } from '@playwright/test';

function loadPuzzle(length, id) {
  const file = path.resolve(process.cwd(), `docs/puzzles/${length}.json`);
  const list = JSON.parse(fs.readFileSync(file, 'utf-8'));
  return list.find((x) => x.id === id);
}

test('promotion prompt buttons are clickable', async ({ page }) => {
  const puzzle = loadPuzzle(5, 1);
  const first = puzzle.solution[0];
  if (!first || !first.promote || !first.from) test.skip();

  await page.goto('/?mate=5&id=1');
  await expect(page.getByRole('heading', { name: '5手詰 #1' })).toBeVisible();

  await page.locator(`button[data-x='${first.from[0]}'][data-y='${first.from[1]}']`).click();
  await page.locator(`button[data-x='${first.to[0]}'][data-y='${first.to[1]}']`).click();

  const prompt = page.getByText('成りますか？');
  await expect(prompt).toBeVisible();

  await page.getByRole('button', { name: '成る' }).click();
  await expect(prompt).not.toBeVisible();
  await expect(page.getByText(/正解。次の一手へ。|クリア！/)).toBeVisible();
});

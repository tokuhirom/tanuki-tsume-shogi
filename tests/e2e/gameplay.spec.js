import fs from 'node:fs';
import path from 'node:path';
import { expect, test } from '@playwright/test';

function loadPuzzle(length, id) {
  const file = path.resolve(process.cwd(), `docs/puzzles/${length}.json`);
  const list = JSON.parse(fs.readFileSync(file, 'utf-8'));
  const p = list.find((x) => x.id === id);
  if (!p) throw new Error(`puzzle not found: ${length} #${id}`);
  return p;
}

async function playMove(page, move) {
  if (move.drop) {
    await page.getByRole('button', { name: new RegExp(`^${move.drop} x`) }).click();
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

test('can open puzzle, play first move, and restore by reload', async ({ page }) => {
  const puzzle = loadPuzzle(3, 1);
  const first = puzzle.solution[0];

  await page.goto('/');
  await page.getByRole('button', { name: '3手詰へ' }).click();
  await expect(page.getByRole('heading', { name: '3手詰 - 問題一覧' })).toBeVisible();

  await page.getByRole('button', { name: '1', exact: true }).click();
  await expect(page.getByRole('heading', { name: '3手詰 #1' })).toBeVisible();
  await expect(page).toHaveURL(/\?mate=3&id=1$/);

  await playMove(page, first);

  await expect(page.getByText(/正解。次の一手へ。|クリア！/)).toBeVisible();

  await page.reload();
  await expect(page.getByRole('heading', { name: '3手詰 #1' })).toBeVisible();
  await expect(page).toHaveURL(/\?mate=3&id=1$/);
});

test('can fully solve 3手詰 #1 and mark clear', async ({ page }) => {
  const puzzle = loadPuzzle(3, 1);

  await page.goto('/?mate=3&id=1');
  await expect(page.getByRole('heading', { name: '3手詰 #1' })).toBeVisible();

  for (let i = 0; i < puzzle.solution.length; i += 2) {
    await playMove(page, puzzle.solution[i]);
  }

  await expect(page.getByText('クリア！ localStorage に記録しました。')).toBeVisible();
  const key = 'tanuki-tsume:v1:clear:3:1';
  await expect.poll(async () => page.evaluate((k) => localStorage.getItem(k), key)).toBe('true');
});

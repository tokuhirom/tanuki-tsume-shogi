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

test('can open puzzle, play first move, and restore by reload', async ({ page }) => {
  const puzzle = loadPuzzle(3, 1);
  const first = puzzle.solution[0];

  await page.goto('/');
  await page.getByRole('button', { name: '3手詰へ' }).click();
  await expect(page.getByRole('heading', { name: '3手詰 - 問題一覧' })).toBeVisible();

  await page.getByRole('button', { name: '1', exact: true }).click();
  await expect(page.getByRole('heading', { name: '3手詰 #1' })).toBeVisible();
  await expect(page).toHaveURL(/\?mate=3&id=1$/);

  if (first.drop) {
    await page.getByRole('button', { name: new RegExp(`^${first.drop} x`) }).click();
    await page.locator(`button[data-x='${first.to[0]}'][data-y='${first.to[1]}']`).click();
  } else {
    await page.locator(`button[data-x='${first.from[0]}'][data-y='${first.from[1]}']`).click();
    await page.locator(`button[data-x='${first.to[0]}'][data-y='${first.to[1]}']`).click();
  }

  const promoteDialog = page.getByText('成りますか？');
  if (await promoteDialog.isVisible().catch(() => false)) {
    await page.getByRole('button', { name: first.promote ? '成る' : '成らない' }).click();
  }

  await expect(page.getByText(/正解。次の一手へ。|クリア！/)).toBeVisible();

  await page.reload();
  await expect(page.getByRole('heading', { name: '3手詰 #1' })).toBeVisible();
  await expect(page).toHaveURL(/\?mate=3&id=1$/);
});

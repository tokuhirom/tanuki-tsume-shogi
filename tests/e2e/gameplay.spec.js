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

function loadAllPuzzles(length) {
  const file = path.resolve(process.cwd(), `docs/puzzles/${length}.json`);
  return JSON.parse(fs.readFileSync(file, 'utf-8'));
}

async function playMove(page, move) {
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

async function solveAllMoves(page, puzzle) {
  for (let i = 0; i < puzzle.solution.length; i += 2) {
    await playMove(page, puzzle.solution[i]);
  }
}

test('can open puzzle, play first move, and restore by reload', async ({ page }) => {
  const puzzle = loadPuzzle(3, 1);
  const first = puzzle.solution[0];

  await page.goto('/');
  await page.getByRole('button', { name: '3手詰へ' }).click();
  await expect(page.getByRole('heading', { name: '3手詰' })).toBeVisible();

  await page.getByRole('button', { name: '1', exact: true }).click();
  await expect(page.getByRole('heading', { name: '3手詰 #1' })).toBeVisible();
  await expect(page).toHaveURL(/\?mate=3&id=1$/);

  await playMove(page, first);

  await expect(page.getByText(/正解！|クリア！/)).toBeVisible();

  await page.reload();
  await expect(page.getByRole('heading', { name: '3手詰 #1' })).toBeVisible();
  await expect(page).toHaveURL(/\?mate=3&id=1$/);
});

test('can fully solve 3手詰 #1 and mark clear', async ({ page }) => {
  const puzzle = loadPuzzle(3, 1);

  await page.goto('/?mate=3&id=1');
  await expect(page.getByRole('heading', { name: '3手詰 #1' })).toBeVisible();

  await solveAllMoves(page, puzzle);

  await expect(page.getByText('クリア！')).toBeVisible();
  const key = 'tanuki-tsume:v1:clear:3:1';
  await expect.poll(async () => page.evaluate((k) => localStorage.getItem(k), key)).toBe('true');
});

test('clear badge and next button shown after solving', async ({ page }) => {
  const puzzle = loadPuzzle(3, 1);

  await page.goto('/?mate=3&id=1');
  await solveAllMoves(page, puzzle);

  await expect(page.locator('.clear-badge')).toBeVisible();
  await expect(page.getByRole('button', { name: '次の問題へ →' })).toBeVisible();
});

test('next puzzle button navigates to next puzzle', async ({ page }) => {
  const puzzle = loadPuzzle(3, 1);

  await page.goto('/?mate=3&id=1');
  await solveAllMoves(page, puzzle);

  await page.getByRole('button', { name: '次の問題へ →' }).click();
  await expect(page.getByRole('heading', { name: '3手詰 #2' })).toBeVisible();
  await expect(page).toHaveURL(/\?mate=3&id=2$/);
});

test('prev/next navigation buttons work', async ({ page }) => {
  await page.goto('/?mate=3&id=2');
  await expect(page.getByRole('heading', { name: '3手詰 #2' })).toBeVisible();

  await page.getByRole('button', { name: '◀ 前' }).click();
  await expect(page.getByRole('heading', { name: '3手詰 #1' })).toBeVisible();

  await page.getByRole('button', { name: '次 ▶' }).click();
  await expect(page.getByRole('heading', { name: '3手詰 #2' })).toBeVisible();
});

test('undo restores previous state', async ({ page }) => {
  const puzzle = loadPuzzle(3, 1);
  const first = puzzle.solution[0];

  await page.goto('/?mate=3&id=1');
  await playMove(page, first);
  await expect(page.getByText(/正解！|クリア！/)).toBeVisible();

  await page.getByRole('button', { name: '↩ 一手戻す' }).click();
  await expect(page.getByText('一手戻しました。')).toBeVisible();
});

test('puzzle list shows clear count', async ({ page }) => {
  const puzzle = loadPuzzle(3, 1);

  await page.goto('/?mate=3&id=1');
  await solveAllMoves(page, puzzle);
  await expect(page.getByText('クリア！')).toBeVisible();

  await page.getByRole('button', { name: '← 一覧' }).click();
  await expect(page.getByText(/クリア: \d+ \/ \d+/)).toBeVisible();
});

test('solution toggle hides/shows solution', async ({ page }) => {
  await page.goto('/?mate=3&id=1');
  await expect(page.getByRole('heading', { name: '3手詰 #1' })).toBeVisible();

  await expect(page.getByText('▶ 手順を表示')).toBeVisible();
  const solutionContent = page.locator('.log', { hasText: '手順を隠す' });
  await expect(solutionContent).not.toBeVisible();

  await page.getByText('▶ 手順を表示').click();
  await expect(page.getByText('▼ 手順を隠す')).toBeVisible();

  await page.getByText('▼ 手順を隠す').click();
  await expect(page.getByText('▶ 手順を表示')).toBeVisible();
});

test('sound toggle persists state', async ({ page }) => {
  await page.goto('/');

  await expect(page.getByRole('button', { name: /音: ON/ })).toBeVisible();
  await page.getByRole('button', { name: /音: ON/ }).click();
  await expect(page.getByRole('button', { name: /音: OFF/ })).toBeVisible();

  const stored = await page.evaluate(() => localStorage.getItem('tanuki-tsume:v1:sound-enabled'));
  expect(stored).toBe('false');
});

test('title screen navigation works', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'たぬき詰将棋' })).toBeVisible();

  await page.getByRole('button', { name: '3手詰へ' }).click();
  await expect(page.getByRole('heading', { name: '3手詰' })).toBeVisible();

  await page.getByRole('button', { name: '← タイトル' }).click();
  await expect(page.getByRole('heading', { name: 'たぬき詰将棋' })).toBeVisible();
});

test('hand pieces show Japanese labels', async ({ page }) => {
  const puzzles = loadAllPuzzles(3);
  const withHand = puzzles.find((p) =>
    p.initial.hands && Object.values(p.initial.hands.attacker || {}).some((c) => c > 0)
  );
  if (!withHand) test.skip();

  await page.goto(`/?mate=3&id=${withHand.id}`);
  await expect(page.getByText('持ち駒')).toBeVisible();
});

test('cleared puzzles shown in green on puzzle list', async ({ page }) => {
  const puzzle = loadPuzzle(3, 1);
  await page.goto('/?mate=3&id=1');
  await solveAllMoves(page, puzzle);

  await page.getByRole('button', { name: '← 一覧' }).click();
  const clearedBtn = page.locator('.puzzle-num.clear').first();
  await expect(clearedBtn).toBeVisible();
});

test('board cells have minimum tap size', async ({ page }) => {
  await page.goto('/?mate=3&id=1');
  const cell = page.locator('.board button').first();
  const box = await cell.boundingBox();
  expect(box.width).toBeGreaterThanOrEqual(40);
  expect(box.height).toBeGreaterThanOrEqual(40);
});

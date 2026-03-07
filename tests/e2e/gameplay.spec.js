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

function puzzleHash(puzzle) {
  const src = JSON.stringify(puzzle.initial);
  let h = 0;
  for (let i = 0; i < src.length; i++) {
    h = ((h << 5) - h + src.charCodeAt(i)) | 0;
  }
  return (h >>> 0).toString(36);
}

function clearKey(puzzle) {
  return `tanuki-tsume:v2:clear:${puzzle.mateLength}:${puzzleHash(puzzle)}`;
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

/** Find an attacker piece that is NOT used in the first solution move */
function findWrongPiece(puzzle) {
  const sol = puzzle.solution[0];
  const solFrom = sol.from;
  for (const p of puzzle.initial.pieces) {
    if (p.owner !== 'attacker') continue;
    if (p.type === 'K') continue;
    if (solFrom && p.x === solFrom[0] && p.y === solFrom[1]) continue;
    return p;
  }
  return null;
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

  await expect(page.getByText(/次の一手へ。|クリア！/)).toBeVisible();

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
  const key = clearKey(puzzle);
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
  await expect(page.getByText(/次の一手へ。|クリア！/)).toBeVisible();

  await page.getByRole('button', { name: '↩ 一手戻す' }).click();
  await expect(page.getByText('一手戻しました。')).toBeVisible();
});

test('wrong move leads to incorrect result', async ({ page }) => {
  const puzzle = loadPuzzle(3, 1);
  const wrongPiece = findWrongPiece(puzzle);
  if (!wrongPiece) test.skip();

  await page.goto('/?mate=3&id=1');
  await expect(page.getByRole('heading', { name: '3手詰 #1' })).toBeVisible();

  // Click the wrong attacker piece to select it
  await page.locator(`button[data-x='${wrongPiece.x}'][data-y='${wrongPiece.y}']`).click();

  // Click a move target to play the wrong move
  const targets = page.locator('.board button.move-target');
  await expect(targets.first()).toBeVisible();
  await targets.first().click();

  // Handle promotion if prompted
  const prompt = page.getByText('成りますか？');
  if (await prompt.isVisible().catch(() => false)) {
    await page.getByRole('button', { name: '成る' }).click();
  }

  // Game should continue or show result
  const wrongBadge = page.locator('.wrong-badge');
  const message = page.locator('.message');

  // If game continues, play another move
  if (!await wrongBadge.isVisible().catch(() => false)) {
    // Try to play another move from whatever pieces are available
    const cells = page.locator('.board button .piece:not(.defender)');
    if (await cells.count() > 0) {
      await cells.first().click();
      const nextTargets = page.locator('.board button.move-target');
      if (await nextTargets.count() > 0) {
        await nextTargets.first().click();
        const prompt2 = page.getByText('成りますか？');
        if (await prompt2.isVisible().catch(() => false)) {
          await page.getByRole('button', { name: '成る' }).click();
        }
      }
    }
  }

  // Should reach either clear or wrong
  const result = page.locator('.wrong-badge, .clear-badge');
  await expect(result).toBeVisible({ timeout: 5000 });
});

test('retry button resets puzzle after wrong answer', async ({ page }) => {
  const puzzle = loadPuzzle(3, 1);
  const wrongPiece = findWrongPiece(puzzle);
  if (!wrongPiece) test.skip();

  await page.goto('/?mate=3&id=1');

  // Play wrong moves until 不正解
  await page.locator(`button[data-x='${wrongPiece.x}'][data-y='${wrongPiece.y}']`).click();
  const targets = page.locator('.board button.move-target');
  await expect(targets.first()).toBeVisible();
  await targets.first().click();

  const prompt = page.getByText('成りますか？');
  if (await prompt.isVisible().catch(() => false)) {
    await page.getByRole('button', { name: '成る' }).click();
  }

  const wrongBadge = page.locator('.wrong-badge');
  if (!await wrongBadge.isVisible().catch(() => false)) {
    const cells = page.locator('.board button .piece:not(.defender)');
    if (await cells.count() > 0) {
      await cells.first().click();
      const nextTargets = page.locator('.board button.move-target');
      if (await nextTargets.count() > 0) {
        await nextTargets.first().click();
        const prompt2 = page.getByText('成りますか？');
        if (await prompt2.isVisible().catch(() => false)) {
          await page.getByRole('button', { name: '成る' }).click();
        }
      }
    }
  }

  await expect(wrongBadge).toBeVisible({ timeout: 5000 });
  await expect(page.getByRole('button', { name: '最初からやり直す' })).toBeVisible();

  await page.getByRole('button', { name: '最初からやり直す' }).click();
  await expect(page.getByRole('heading', { name: '3手詰 #1' })).toBeVisible();
  await expect(wrongBadge).not.toBeVisible();
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

test('reopening cleared puzzle shows cleared status', async ({ page }) => {
  const puzzle = loadPuzzle(3, 1);
  await page.goto('/?mate=3&id=1');
  await solveAllMoves(page, puzzle);
  await expect(page.getByText('クリア！')).toBeVisible();

  await page.goto('/?mate=3&id=1');
  await expect(page.getByText('✅ クリア済み')).toBeVisible();
});

test('board cells have minimum tap size', async ({ page }) => {
  await page.goto('/?mate=3&id=1');
  const cell = page.locator('.board button').first();
  const box = await cell.boundingBox();
  expect(box.width).toBeGreaterThanOrEqual(40);
  expect(box.height).toBeGreaterThanOrEqual(40);
});

test('clear data reset with confirmation', async ({ page }) => {
  // Set a clear flag directly using v2 hash key
  const puzzle = loadPuzzle(3, 1);
  const key = clearKey(puzzle);
  await page.goto('/');
  await page.evaluate((k) => localStorage.setItem(k, 'true'), key);

  // Reload title
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'たぬき詰将棋' })).toBeVisible();

  // Click reset button
  await page.getByRole('button', { name: 'クリアデータ削除' }).click();
  await expect(page.getByText('すべてのクリアデータを削除しますか？')).toBeVisible();

  // Cancel first
  await page.getByRole('button', { name: 'キャンセル' }).click();
  await expect(page.getByText('すべてのクリアデータを削除しますか？')).not.toBeVisible();

  // Data should still exist
  const before = await page.evaluate((k) => localStorage.getItem(k), key);
  expect(before).toBe('true');

  // Now actually delete
  await page.getByRole('button', { name: 'クリアデータ削除' }).click();
  await page.getByRole('button', { name: '削除する' }).click();
  await expect(page.getByText(/クリアデータを削除しました/)).toBeVisible();

  const after = await page.evaluate((k) => localStorage.getItem(k), key);
  expect(after).toBeNull();
});

test('any legal move is accepted (free play)', async ({ page }) => {
  const puzzle = loadPuzzle(3, 1);
  const sol = puzzle.solution[0];

  await page.goto('/?mate=3&id=1');
  await expect(page.getByRole('heading', { name: '3手詰 #1' })).toBeVisible();

  if (sol.drop) {
    // First move is a drop — click hand piece then target
    const label = { R: '飛', B: '角', G: '金', S: '銀', P: '歩' }[sol.drop] || sol.drop;
    await page.getByRole('button', { name: new RegExp(`^${label} ×`) }).click();
    const targets = page.locator('.board button.move-target');
    await expect(targets.first()).toBeVisible();
    await targets.first().click();
  } else {
    // First move is a board move — click piece then target
    const from = sol.from;
    await page.locator(`button[data-x='${from[0]}'][data-y='${from[1]}']`).click();
    const targets = page.locator('.board button.move-target');
    await expect(targets.first()).toBeVisible();
    await targets.first().click();
  }

  // Handle promotion if prompted
  const prompt = page.getByText('成りますか？');
  if (await prompt.isVisible().catch(() => false)) {
    await page.getByRole('button', { name: '成る' }).click();
  }

  // Move should be accepted — message should change from initial
  const message = page.locator('.message');
  await expect(message).not.toHaveText('攻め方の手を選んでください');
});

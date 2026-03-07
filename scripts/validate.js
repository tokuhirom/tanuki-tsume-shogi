import fs from "node:fs";
import { createState, validateTsumePuzzle } from "../src/shogi-core.js";

const lengths = [3, 5, 7, 9];
let failed = 0;

for (const n of lengths) {
  const file = `puzzles/${n}.json`;
  if (!fs.existsSync(file)) {
    console.error(`[NG] ${file} が存在しません`);
    failed += 1;
    continue;
  }
  const puzzles = JSON.parse(fs.readFileSync(file, "utf-8"));
  for (const p of puzzles) {
    const st = createState(p.initial);
    const res = validateTsumePuzzle(st, n);
    if (!res.ok) {
      console.error(`[NG] ${n}手詰 #${p.id}: ${res.reason}`);
      failed += 1;
      break;
    }
  }
  if (failed === 0) {
    console.log(`[OK] ${file}: ${puzzles.length}問`);
  }
}

if (failed > 0) {
  process.exit(1);
}

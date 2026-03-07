import fs from "node:fs";

function parseArgs() {
  const args = process.argv.slice(2);
  const out = { file: "", source: "" };
  for (const a of args) {
    if (a.startsWith("--file=")) out.file = a.split("=")[1];
    if (a.startsWith("--source=")) out.source = a.split("=")[1];
  }
  return out;
}

function loadJson(file) {
  return JSON.parse(fs.readFileSync(file, "utf-8"));
}

function main() {
  const { file } = parseArgs();
  if (!file || !fs.existsSync(file)) {
    throw new Error("--file=<mined-json> が必要です");
  }

  const mined = loadJson(file);
  const mateLength = Number(mined.mateLength);
  if (![3, 5, 7, 9].includes(mateLength)) {
    throw new Error("mateLength must be one of 3/5/7/9");
  }
  if (!mined.initial) {
    throw new Error("mined json must include initial");
  }

  const curatedFile = "data/curated-puzzles.json";
  const curated = loadJson(curatedFile);
  const key = String(mateLength);
  curated[key] = curated[key] || [];

  const sig = JSON.stringify(mined.initial);
  const exists = curated[key].some((p) => JSON.stringify(p) === sig);
  if (exists) {
    console.log("already exists");
    return;
  }

  curated[key].push(mined.initial);
  fs.writeFileSync(curatedFile, JSON.stringify(curated, null, 2), "utf-8");
  console.log(`added mate=${mateLength} total=${curated[key].length}`);
}

main();

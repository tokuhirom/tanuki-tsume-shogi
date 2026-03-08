import { defineConfig } from "vite";
import wasm from "vite-plugin-wasm";
import { execSync } from "node:child_process";

function safe(cmd, fallback = "unknown") {
  try {
    return execSync(cmd, { stdio: ["ignore", "pipe", "ignore"] }).toString().trim();
  } catch {
    return fallback;
  }
}

const branch = safe("git branch --show-current");
const commit = safe("git rev-parse --short HEAD");
const builtAt = new Date().toISOString();

export default defineConfig({
  base: "/tanuki-tsume-shogi/",
  plugins: [wasm()],
  define: {
    __BUILD_INFO__: JSON.stringify({ branch, commit, builtAt }),
  },
  build: {
    outDir: "dist",
    target: "esnext",
  },
  worker: {
    format: "es",
  },
  test: {
    include: ["tests/unit/**/*.test.js"],
  },
});

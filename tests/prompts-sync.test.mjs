import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { AI_TASKS } from '../ai.js';

// Rust の CLI（cli/src/tasks.rs）のプロンプトが、拡張機能の ai.js と一字一句同じであることを確かめる。
const tasksRs = readFileSync(new URL('../cli/src/tasks.rs', import.meta.url), 'utf8');

function rustLiteral(text) {
  return JSON.stringify(text);
}

for (const [name, task] of Object.entries(AI_TASKS)) {
  test(`${name} のプロンプトが cli/src/tasks.rs と一致する`, () => {
    const declaration = `pub const ${name.toUpperCase()}_PROMPT: &str = ${rustLiteral(task.system)};`;
    assert.ok(tasksRs.includes(declaration), `${name.toUpperCase()}_PROMPT が ai.js と異なります`);
  });
}

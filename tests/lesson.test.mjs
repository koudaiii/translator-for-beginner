import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

const require = createRequire(import.meta.url);
const Lesson = require('../lesson.js');

function recorder() {
  const events = [];
  return {
    events,
    ui: {
      translation: (res) => events.push(['translation', res]),
      aiStarted: () => events.push(['aiStarted']),
      ai: (task, res) => events.push(['ai', task, res]),
    },
  };
}

function fakeSend(handlers) {
  const sent = [];
  const send = (msg) => {
    sent.push(msg);
    const handler = handlers[msg.type];
    return handler ? handler(msg) : Promise.resolve(undefined);
  };
  return { send, sent };
}

const ok = (data) => Promise.resolve({ ok: true, data });

test('hasKana はひらがなとカタカナを検出する', () => {
  assert.equal(Lesson.hasKana('これはペンです'), true);
  assert.equal(Lesson.hasKana('カタカナ'), true);
  assert.equal(Lesson.hasKana('This is a pen.'), false);
  assert.equal(Lesson.hasKana('東京'), false);
});

test('英文なら翻訳と 4 つの AI 処理をすべて依頼する', async () => {
  const { send, sent } = fakeSend({
    translate: () => Promise.resolve({ ok: true, text: '訳', detected: 'en' }),
    aiAvailable: () => Promise.resolve(true),
    reading: () => ok({ literal: 'x', vocabulary: [] }),
    grammar: () => ok({ grammar: [] }),
    corrections: () => ok({ corrections: [] }),
    variants: () => ok({ chat: 'a', email: 'b', spoken: 'c', github: 'd' }),
  });
  const { events, ui } = recorder();
  await Lesson.startLesson('I can not do it.', send, ui);

  assert.deepEqual(sent.map((m) => m.type), ['translate', 'aiAvailable', 'reading', 'grammar', 'corrections', 'variants']);
  for (const m of sent) if (m.type !== 'aiAvailable') assert.equal(m.text, 'I can not do it.');
  assert.deepEqual(events.find((e) => e[0] === 'translation'), ['translation', { ok: true, text: '訳', detected: 'en' }]);
  assert.ok(events.some((e) => e[0] === 'aiStarted'));
  assert.deepEqual(events.filter((e) => e[0] === 'ai').map((e) => e[1]).sort(), ['corrections', 'grammar', 'reading', 'variants']);
});

test('AI 処理は翻訳の完了を待たずに依頼する', async () => {
  let resolveTranslate;
  const { send, sent } = fakeSend({
    translate: () => new Promise((r) => { resolveTranslate = r; }),
    aiAvailable: () => Promise.resolve(true),
  });
  const { ui } = recorder();
  const done = Lesson.startLesson('Hello.', send, ui);
  await new Promise((r) => setTimeout(r, 0));
  assert.deepEqual(sent.map((m) => m.type), ['translate', 'aiAvailable', 'reading', 'grammar', 'corrections', 'variants']);
  resolveTranslate({ ok: true, text: 'こんにちは', detected: 'en' });
  await done;
});

test('先に届いた結果から表示する', async () => {
  let resolveGrammar;
  const { send } = fakeSend({
    translate: () => Promise.resolve({ ok: true, text: '訳', detected: 'en' }),
    aiAvailable: () => Promise.resolve(true),
    grammar: () => new Promise((r) => { resolveGrammar = r; }),
    reading: () => ok({ literal: 'x', vocabulary: [] }),
    corrections: () => ok({ corrections: [] }),
    variants: () => ok({ chat: 'a', email: 'b', spoken: 'c', github: 'd' }),
  });
  const { events, ui } = recorder();
  const done = Lesson.startLesson('Hello.', send, ui);
  await new Promise((r) => setTimeout(r, 0));
  assert.equal(events.filter((e) => e[0] === 'ai').length, 3);
  resolveGrammar({ ok: true, data: { grammar: [] } });
  await done;
  assert.deepEqual(events.at(-1).slice(0, 2), ['ai', 'grammar']);
});

test('ひらがな・カタカナを含む文では AI 処理を依頼しない', async () => {
  const { send, sent } = fakeSend({
    translate: () => Promise.resolve({ ok: true, text: 'これはペンです', detected: 'ja' }),
    aiAvailable: () => Promise.resolve(true),
  });
  const { events, ui } = recorder();
  await Lesson.startLesson('これはペンです', send, ui);
  assert.deepEqual(sent.map((m) => m.type), ['translate']);
  assert.ok(!events.some((e) => e[0] === 'aiStarted'));
});

test('Azure OpenAI が未設定なら翻訳だけを依頼する', async () => {
  const { send, sent } = fakeSend({
    translate: () => Promise.resolve({ ok: true, text: '訳', detected: 'en' }),
    aiAvailable: () => Promise.resolve(false),
  });
  const { events, ui } = recorder();
  await Lesson.startLesson('Hello.', send, ui);
  assert.deepEqual(sent.map((m) => m.type), ['translate', 'aiAvailable']);
  assert.ok(!events.some((e) => e[0] === 'aiStarted'));
});

test('応答が undefined でも「作成中」のまま止まらずメッセージになる', async () => {
  const { send } = fakeSend({
    translate: () => Promise.resolve(undefined),
    aiAvailable: () => Promise.resolve(true),
  });
  const { events, ui } = recorder();
  await Lesson.startLesson('Hello.', send, ui);
  assert.deepEqual(events.find((e) => e[0] === 'translation'), ['translation', { ok: false, error: '翻訳に失敗しました' }]);
  const aiEvents = events.filter((e) => e[0] === 'ai');
  assert.equal(aiEvents.length, 4);
  for (const [, , res] of aiEvents) assert.deepEqual(res, { ok: false, error: '結果を受け取れませんでした' });
});

test('1 つが例外で失敗しても他の結果は表示する', async () => {
  const { send } = fakeSend({
    translate: () => Promise.reject(new Error('boom')),
    aiAvailable: () => Promise.resolve(true),
    reading: () => Promise.reject(new Error('down')),
    grammar: () => ok({ grammar: [] }),
    corrections: () => ok({ corrections: [] }),
    variants: () => ok({ chat: 'a', email: 'b', spoken: 'c', github: 'd' }),
  });
  const { events, ui } = recorder();
  await Lesson.startLesson('Hello.', send, ui);
  assert.deepEqual(events.find((e) => e[0] === 'translation'), ['translation', { ok: false, error: '翻訳に失敗しました: boom' }]);
  assert.deepEqual(events.find((e) => e[1] === 'reading'), ['ai', 'reading', { ok: false, error: '通信に失敗しました: down' }]);
  assert.equal(events.find((e) => e[1] === 'grammar')[2].ok, true);
});

test('aiAvailable が例外になっても翻訳は表示する', async () => {
  const { send } = fakeSend({
    translate: () => Promise.resolve({ ok: true, text: '訳', detected: 'en' }),
    aiAvailable: () => Promise.reject(new Error('down')),
  });
  const { events, ui } = recorder();
  await Lesson.startLesson('Hello.', send, ui);
  assert.equal(events.find((e) => e[0] === 'translation')[1].ok, true);
  assert.ok(!events.some((e) => e[0] === 'aiStarted'));
});

test('同じ環境に 2 回注入してもエラーにならず、最初の定義を使う', () => {
  const code = readFileSync(new URL('../lesson.js', import.meta.url), 'utf8');
  const context = vm.createContext({});
  vm.runInContext(code, context);
  const first = context.Lesson;
  assert.equal(typeof first.startLesson, 'function');
  vm.runInContext(code, context);
  assert.equal(context.Lesson, first);
});

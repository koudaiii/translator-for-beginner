import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  MAX_AI_CHARS, AI_TASKS, buildAoaiRequest, parseAoaiResponse,
  normalizeEndpoint, isValidEndpoint, isAiConfigured, runAiTask,
} from '../ai.js';

const SETTINGS = {
  aoaiEndpoint: 'https://example.openai.azure.com',
  aoaiKey: 'test-key',
  aoaiDeployment: 'gpt-6-luna',
};

function completion(content, finishReason = 'stop') {
  return { choices: [{ finish_reason: finishReason, message: { content } }] };
}

function fakeFetch(response) {
  const calls = [];
  const impl = async (url, init) => {
    calls.push({ url, init });
    if (response instanceof Error) throw response;
    return response;
  };
  return { impl, calls };
}

function jsonResponse(body, status = 200) {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}

function assertStrictSchema(schema, path = 'schema') {
  if (schema.type === 'object') {
    assert.equal(schema.additionalProperties, false, `${path} additionalProperties`);
    assert.deepEqual([...schema.required].sort(), Object.keys(schema.properties).sort(), `${path} required`);
    for (const [key, value] of Object.entries(schema.properties)) assertStrictSchema(value, `${path}.${key}`);
  }
  if (schema.type === 'array') assertStrictSchema(schema.items, `${path}[]`);
}

test('4 つの処理が定義されている', () => {
  assert.deepEqual(Object.keys(AI_TASKS), ['reading', 'grammar', 'corrections', 'variants']);
});

test('すべてのスキーマが strict モードの要件を満たす', () => {
  for (const [name, task] of Object.entries(AI_TASKS)) assertStrictSchema(task.schema, name);
});

test('件数の制約がスキーマに入っている', () => {
  const vocab = AI_TASKS.reading.schema.properties.vocabulary;
  assert.equal(vocab.maxItems, 5);
  const grammar = AI_TASKS.grammar.schema.properties.grammar;
  assert.equal(grammar.minItems, 1);
  assert.equal(grammar.maxItems, 3);
  const examples = grammar.items.properties.examples;
  assert.equal(examples.minItems, 2);
  assert.equal(examples.maxItems, 2);
  assert.equal(AI_TASKS.corrections.schema.properties.corrections.maxItems, 5);
  assert.deepEqual(Object.keys(AI_TASKS.variants.schema.properties), ['chat', 'email', 'spoken', 'github']);
});

test('文法の explanation に例文を書かせない', () => {
  assert.match(AI_TASKS.grammar.system, /explanation には例文や examples を書かない/);
});

test('buildAoaiRequest がモデル、推論レベル、スキーマ、入力を組み立てる', () => {
  const body = buildAoaiRequest('variants', 'I can not do it.', 'gpt-6-luna');
  assert.equal(body.model, 'gpt-6-luna');
  assert.equal(body.reasoning_effort, 'none');
  assert.equal(body.messages[0].role, 'system');
  assert.equal(body.messages[0].content, AI_TASKS.variants.system);
  assert.deepEqual(body.messages[1], { role: 'user', content: 'I can not do it.' });
  assert.deepEqual(body.response_format, {
    type: 'json_schema',
    json_schema: { name: 'variants', strict: true, schema: AI_TASKS.variants.schema },
  });
});

test('解説系の処理は入力に「英文: 」を付ける', () => {
  assert.equal(buildAoaiRequest('reading', 'Hi.', 'd').messages[1].content, '英文: Hi.');
  assert.equal(buildAoaiRequest('grammar', 'Hi.', 'd').messages[1].content, '英文: Hi.');
  assert.equal(buildAoaiRequest('corrections', 'Hi.', 'd').messages[1].content, 'Hi.');
});

test('parseAoaiResponse は finish_reason が stop の JSON を返す', () => {
  assert.deepEqual(parseAoaiResponse(completion('{"chat":"a"}')), { ok: true, data: { chat: 'a' } });
});

test('parseAoaiResponse は途中で切れた応答、壊れた JSON、空の応答を失敗にする', () => {
  const error = { ok: false, error: '結果を読み取れませんでした' };
  assert.deepEqual(parseAoaiResponse(completion('{"chat":"a"}', 'length')), error);
  assert.deepEqual(parseAoaiResponse(completion('{"chat":')), error);
  assert.deepEqual(parseAoaiResponse({}), error);
  assert.deepEqual(parseAoaiResponse(null), error);
});

test('normalizeEndpoint は空白、末尾の /、/openai/v1 を取り除く', () => {
  assert.equal(normalizeEndpoint(' https://x.openai.azure.com/ '), 'https://x.openai.azure.com');
  assert.equal(normalizeEndpoint('https://x.openai.azure.com/openai/v1/'), 'https://x.openai.azure.com');
  assert.equal(normalizeEndpoint('https://x.openai.azure.com/openai/v1'), 'https://x.openai.azure.com');
  assert.equal(normalizeEndpoint('https://x.openai.azure.com'), 'https://x.openai.azure.com');
});

test('isValidEndpoint は https の openai.azure.com だけを受け付ける', () => {
  assert.equal(isValidEndpoint('https://my-resource.openai.azure.com'), true);
  assert.equal(isValidEndpoint('https://x.cognitiveservices.azure.com'), false);
  assert.equal(isValidEndpoint('http://x.openai.azure.com'), false);
  assert.equal(isValidEndpoint('https://x.openai.azure.com/openai/v1'), false);
  assert.equal(isValidEndpoint(''), false);
});

test('isAiConfigured は 3 項目がすべてあるときだけ true', () => {
  assert.equal(isAiConfigured(SETTINGS), true);
  assert.equal(isAiConfigured({ ...SETTINGS, aoaiKey: '' }), false);
  assert.equal(isAiConfigured({}), false);
});

test('runAiTask は未設定なら呼び出さずにメッセージを返す', async () => {
  const { impl, calls } = fakeFetch(jsonResponse(completion('{}')));
  const res = await runAiTask('variants', 'Hi.', {}, impl);
  assert.deepEqual(res, { ok: false, error: '設定ページで Azure OpenAI を登録してください' });
  assert.equal(calls.length, 0);
});

test('runAiTask は 1000 文字を超えると呼び出さない', async () => {
  const { impl, calls } = fakeFetch(jsonResponse(completion('{}')));
  const res = await runAiTask('variants', 'a'.repeat(MAX_AI_CHARS + 1), SETTINGS, impl);
  assert.deepEqual(res, { ok: false, error: '解説・添削は 1000 文字までです。短く選択してください' });
  assert.equal(calls.length, 0);
});

test('runAiTask はちょうど 1000 文字なら呼び出す', async () => {
  const { impl, calls } = fakeFetch(jsonResponse(completion('{"chat":"a","email":"b","spoken":"c","github":"d"}')));
  const res = await runAiTask('variants', 'a'.repeat(MAX_AI_CHARS), SETTINGS, impl);
  assert.equal(res.ok, true);
  assert.equal(calls.length, 1);
});

test('runAiTask は v1 の URL と api-key ヘッダで呼び出し、データを返す', async () => {
  const data = { corrections: [{ before: 'can not', after: 'cannot', reason: '1 語で書く' }] };
  const { impl, calls } = fakeFetch(jsonResponse(completion(JSON.stringify(data))));
  const res = await runAiTask('corrections', 'I can not.', SETTINGS, impl);
  assert.deepEqual(res, { ok: true, data });
  assert.equal(calls[0].url, 'https://example.openai.azure.com/openai/v1/chat/completions');
  assert.equal(calls[0].init.method, 'POST');
  assert.equal(calls[0].init.headers['api-key'], 'test-key');
  assert.equal(calls[0].init.headers['Content-Type'], 'application/json');
  assert.deepEqual(JSON.parse(calls[0].init.body), buildAoaiRequest('corrections', 'I can not.', 'gpt-6-luna'));
});

test('runAiTask は HTTP エラーをステータスと本文付きで返す', async () => {
  const { impl } = fakeFetch(new Response('{"error":"Access denied"}', { status: 401 }));
  const res = await runAiTask('variants', 'Hi.', SETTINGS, impl);
  assert.deepEqual(res, { ok: false, error: 'Azure OpenAI がエラーを返しました (401) {"error":"Access denied"}' });
});

test('runAiTask は通信失敗をメッセージにする', async () => {
  const { impl } = fakeFetch(new TypeError('Failed to fetch'));
  const res = await runAiTask('variants', 'Hi.', SETTINGS, impl);
  assert.deepEqual(res, { ok: false, error: '通信に失敗しました: Failed to fetch' });
});

test('runAiTask は応答本文が JSON でなければ失敗にする', async () => {
  const { impl } = fakeFetch(new Response('not json', { status: 200 }));
  const res = await runAiTask('variants', 'Hi.', SETTINGS, impl);
  assert.deepEqual(res, { ok: false, error: '結果を読み取れませんでした' });
});

test('runAiTask は不明な処理名を拒否する', async () => {
  const { impl, calls } = fakeFetch(jsonResponse(completion('{}')));
  const res = await runAiTask('translate', 'Hi.', SETTINGS, impl);
  assert.deepEqual(res, { ok: false, error: '不明な処理です' });
  assert.equal(calls.length, 0);
});

test('parseAoaiResponse は回答拒否や null の内容を失敗にする', () => {
  const error = { ok: false, error: '結果を読み取れませんでした' };
  assert.deepEqual(parseAoaiResponse(completion(null)), error);
  assert.deepEqual(parseAoaiResponse(completion('null')), error);
  assert.deepEqual(parseAoaiResponse(completion('"text"')), error);
  assert.deepEqual(parseAoaiResponse({ choices: [{ finish_reason: 'stop', message: { content: null, refusal: 'I cannot help with that.' } }] }), error);
});

test('reading の単語・熟語は発音記号とカタカナを必ず含む', () => {
  const item = AI_TASKS.reading.schema.properties.vocabulary.items;
  assert.deepEqual(Object.keys(item.properties), ['term', 'meaning', 'ipa', 'katakana']);
  assert.deepEqual(item.required, ['term', 'meaning', 'ipa', 'katakana']);
  assert.match(AI_TASKS.reading.system, /ipa はアメリカ英語の発音記号/);
  assert.match(AI_TASKS.reading.system, /katakana は実際の音に近いカタカナ/);
});

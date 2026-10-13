export const MAX_AI_CHARS = 1000;

const NOT_CONFIGURED = '設定ページで Azure OpenAI を登録してください';
const LIMIT_ERROR = `解説・添削は ${MAX_AI_CHARS} 文字までです。短く選択してください`;
const READ_ERROR = '結果を読み取れませんでした';

const TEACHER = 'あなたは英語初心者の日本人に教える英語教師です。与えられた英文について、日本語で簡潔に解説してください。\n';
const EDITOR = 'あなたは英語初心者の日本人が書いた英文を直す英語の編集者です。';

const str = { type: 'string' };

function obj(properties) {
  return { type: 'object', additionalProperties: false, required: Object.keys(properties), properties };
}

function list(items, limits) {
  return { type: 'array', ...limits, items };
}

export const AI_TASKS = {
  reading: {
    system: TEACHER
      + '- literal: 英語の語順のまま、意味のまとまりごとに「 / 」で区切った日本語の直訳。英単語は書かない。例: 「私は / ずっと住んでいる / 東京に / 〜以来 / 私が / 卒業した / 大学を」\n'
      + '- vocabulary: 初心者がつまずきそうな単語・熟語を最大5個。固有名詞や中学1年レベルの基本語は入れない。meaning は文中での意味。',
    user: (text) => `英文: ${text}`,
    schema: obj({
      literal: str,
      vocabulary: list(obj({ term: str, meaning: str }), { maxItems: 5 }),
    }),
  },
  grammar: {
    system: TEACHER
      + '- grammar: 文中の重要な文法項目を1〜3個。point は項目名、explanation は2〜3文の説明、examples には同じ文法を使った短い英文（en）と和訳（ja）を必ず2つ入れる。空にしない。',
    user: (text) => `英文: ${text}`,
    schema: obj({
      grammar: list(obj({
        point: str,
        explanation: str,
        examples: list(obj({ en: str, ja: str }), { minItems: 2, maxItems: 2 }),
      }), { minItems: 1, maxItems: 3 }),
    }),
  },
  corrections: {
    system: EDITOR
      + '入力の英文の文法・語法の誤りを1つずつ挙げる。before は元の表現、after は直した表現、reason は初心者向けの短い日本語の理由。誤りがなければ空配列にする（言い回しの好みは誤りに含めない）。',
    user: (text) => text,
    schema: obj({
      corrections: list(obj({ before: str, after: str, reason: str }), { maxItems: 5 }),
    }),
  },
  variants: {
    system: EDITOR
      + '入力の英文の文法・語法の誤りを直し、意味を変えずに、次の4つの用途に合う英文をそれぞれ1つ書いてください。用途ごとに語調・長さ・言い回しをはっきり変え、同じ文を繰り返さない。情報を足したり削ったりしない。\n'
      + '- chat: Slack や Teams のチャット。短く、フレンドリーな口語。短縮形を使い、文を削ってよい（例: "Checked your PR — looks good!"）。挨拶や署名は付けない。\n'
      + '- email: 仕事のメール本文。完全な文で丁寧に。短縮形を避け、丁寧な定型表現を使う（例: "I apologize for the delayed response."）。入力に挨拶や署名がなければ付けない。\n'
      + '- spoken: 対面や通話で口に出して言う英語。言いやすい短い文に分け、自然なつなぎ言葉を使う（例: "So, ...", "Actually, ..."）。\n'
      + '- github: Issue や Pull Request のコメント。簡潔で明確、中立的な技術文。開発者の慣用表現を使ってよい（例: LGTM, repro）。コード・ファイル名・コマンドはバッククォートで囲む。',
    user: (text) => text,
    schema: obj({ chat: str, email: str, spoken: str, github: str }),
  },
};

export function buildAoaiRequest(task, text, deployment) {
  const { system, user, schema } = AI_TASKS[task];
  return {
    model: deployment,
    reasoning_effort: 'none',
    messages: [
      { role: 'system', content: system },
      { role: 'user', content: user(text) },
    ],
    response_format: { type: 'json_schema', json_schema: { name: task, strict: true, schema } },
  };
}

export function parseAoaiResponse(json) {
  const choice = json && Array.isArray(json.choices) ? json.choices[0] : null;
  if (!choice || choice.finish_reason !== 'stop' || !choice.message) return { ok: false, error: READ_ERROR };
  try {
    return { ok: true, data: JSON.parse(choice.message.content) };
  } catch {
    return { ok: false, error: READ_ERROR };
  }
}

export function normalizeEndpoint(value) {
  return value.trim().replace(/\/+$/, '').replace(/\/openai\/v1$/, '');
}

export function isValidEndpoint(value) {
  return /^https:\/\/[a-z0-9-]+\.openai\.azure\.com$/i.test(value);
}

export function isAiConfigured(settings) {
  return Boolean(settings && settings.aoaiEndpoint && settings.aoaiKey && settings.aoaiDeployment);
}

export async function runAiTask(task, text, settings, fetchImpl = fetch) {
  if (!Object.hasOwn(AI_TASKS, task)) return { ok: false, error: '不明な処理です' };
  if (!isAiConfigured(settings)) return { ok: false, error: NOT_CONFIGURED };
  if (text.length > MAX_AI_CHARS) return { ok: false, error: LIMIT_ERROR };

  let res;
  try {
    res = await fetchImpl(`${settings.aoaiEndpoint}/openai/v1/chat/completions`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'api-key': settings.aoaiKey },
      body: JSON.stringify(buildAoaiRequest(task, text, settings.aoaiDeployment)),
    });
  } catch (e) {
    return { ok: false, error: `通信に失敗しました: ${e.message}` };
  }
  if (!res.ok) {
    const detail = await res.text().catch(() => '');
    return { ok: false, error: `Azure OpenAI がエラーを返しました (${res.status}) ${detail}`.trim() };
  }
  let json;
  try {
    json = await res.json();
  } catch {
    return { ok: false, error: READ_ERROR };
  }
  return parseAoaiResponse(json);
}

# 文法解説（学習モード）設計

## 目的
翻訳した英文について、文法の解説と例文を同じポップアップで読めるようにし、翻訳しながら英語を学べるようにする。利用者は作者本人のみ。

## 前提
- Azure Translator では文法解説を生成できないため、Azure OpenAI（Foundry）のモデルを使う。
- リソース: サブスクリプション `sandbox`、リソースグループ `rg-translator-for-beginner`、リージョン `japaneast`、AIServices リソース `translator-for-beginner-foundry`（S0）。
- デプロイ: `gpt-5-mini`（2025-08-07、GlobalStandard、容量 10 = 10K TPM）。
- API: Azure OpenAI v1 の Chat Completions（`POST https://translator-for-beginner-foundry.openai.azure.com/openai/v1/chat/completions`、ヘッダ `api-key`）。`api-version` は付けない。
- 認証はキー。Entra ID は使わない。
- 選択テキストと訳文は Microsoft のサーバーに送信される。

## 事前検証の結果（2026-10-09）
本設計と同じスキーマとプロンプトで 1 文を送信した。HTTP 200、応答時間 約 5.3 秒、入力 293 / 出力 538 トークン、推論トークン 0（`reasoning_effort: "minimal"`）。スキーマどおりの JSON が返った。`literal` が日本語にならず英文を区切っただけになったこと、固有名詞が単語一覧に入ったことの 2 点は、プロンプトの指示を具体化して対処する。

## 使い方
1. 翻訳はこれまでどおり。訳文の下に「文法を学ぶ」ボタンを表示する。
2. Azure OpenAI が未設定ならボタンを表示しない。
3. ボタンを押すと「解説を作成中…」と表示し、応答が来たら訳文の下に解説を展開する。ボタンは消す。
4. 解説は、ページ上のポップアップ（`content.js`）とツールバーのポップアップ（`action.html`）の両方で使える。

## 構成の変更
- `manifest.json`: `host_permissions` に `https://*.openai.azure.com/*` を追加。バージョンを `0.6.0` に上げる。
- `options.html` / `options.js`: 「Azure OpenAI（文法解説）」欄を追加し、`aoaiEndpoint`（例: `https://translator-for-beginner-foundry.openai.azure.com`）、`aoaiKey`、`aoaiDeployment`（既定値 `gpt-5-mini`）を `chrome.storage.local` に保存する。エンドポイントは末尾の `/` を除いて保存する。
- `background.js`:
  - `explain` メッセージを受け、Azure OpenAI を呼んで解説 JSON を返す。
  - `explainAvailable` メッセージを受け、Azure OpenAI の 3 項目がすべて設定済みかを真偽値で返す（キーそのものは返さない）。
  - 送信元が自分の拡張機能でなければ応答しない（既存の `translate` と同じ）。
- `content.js`: 訳文の表示後にボタンを置き、解説を描画する。描画関数はこのファイル内に持つ（注入スクリプトのため）。
- `action.html` / `action.js`: 同じボタンと解説表示を追加する。描画関数は `action.js` 内に持つ。
- `popup.css`: 解説用のスタイルと、ポップアップの最大の高さ（`max-height: 60vh` 程度）と縦スクロールを追加する。

`content.js` と `action.js` に描画処理が重複するが、どちらも 40 行程度に収まる見込みで、共通化のために注入ファイルを増やすより単純さを優先する。

## リクエスト
```json
{
  "model": "<aoaiDeployment>",
  "reasoning_effort": "minimal",
  "messages": [
    { "role": "system", "content": "<システムプロンプト>" },
    { "role": "user", "content": "英文: <原文>\n和訳: <訳文>" }
  ],
  "response_format": { "type": "json_schema", "json_schema": { "name": "grammar_lesson", "strict": true, "schema": "<下記>" } }
}
```

### システムプロンプトの要点
- 英語初心者の日本人に教える英語教師として、日本語で簡潔に解説する。
- `literal`: 英語の語順のまま、意味のまとまりごとに ` / ` で区切った**日本語**の直訳（例: 「私は / ずっと住んでいる / 東京に / 〜以来 / 私が / 卒業した / 大学を」）。英語を書かない。
- `grammar`: 文中の重要な文法項目を 1〜3 個。`point` は項目名、`explanation` は 2〜3 文の説明、`examples` は同じ文法を使った短い英文と和訳を 2 つ。
- `vocabulary`: 初心者がつまずきそうな単語・熟語を最大 5 個。固有名詞や中学 1 年レベルの基本語は入れない。`meaning` は文中での意味。

### スキーマ
```
{
  literal: string,
  grammar: [{ point: string, explanation: string, examples: [{ en: string, ja: string }] }],
  vocabulary: [{ term: string, meaning: string }]
}
```
すべてのオブジェクトに `additionalProperties: false` と全プロパティの `required` を付ける（strict モードの要件）。

## 表示
訳文の下に区切り線を入れ、次の順で表示する。
1. **直訳**: `literal`
2. **文法ポイント**: 各項目について、見出し `point`、本文 `explanation`、例文（英文と、その下に小さく和訳）
3. **単語・熟語**: `term` — `meaning` の一覧。空なら見出しごと出さない。

すべて `textContent` で組み立て、HTML として解釈させない。

## 上限
原文が 1000 文字を超える場合は LLM を呼ばず、「解説は 1000 文字までです。短く選択してください」と表示する。判定は `background.js` で行う（`content.js` と `action.js` の両方を守るため）。

## エラー処理
解説欄にメッセージを出し、訳文は残す。
- 未設定: 「設定ページで Azure OpenAI を登録してください」
- HTTP エラー: 「Azure OpenAI がエラーを返しました (<status>) <本文>」。401 はキー誤り、404 はデプロイ名かエンドポイントの誤り、429 はレート制限であることが本文から分かる。
- 通信失敗: 「通信に失敗しました: <message>」
- `finish_reason` が `stop` 以外、または JSON の解析失敗: 「解説を読み取れませんでした」

## セキュリティ
- Azure OpenAI のキーは `chrome.storage.local` に保存し、service worker の中だけで使う。`content.js`、`action.js`、ページには渡さない。
- LLM の出力は `textContent` で表示する。
- キーはコードやチャットに書かない。漏れた場合は Azure ポータルで再生成する。

## 対象外
解説の履歴や単語帳の保存、ストリーミング表示、Entra ID 認証、Translator を使わず LLM だけで翻訳すること、解説の言語や詳しさの切り替え。

## テスト（手動）
- 設定ページで Azure OpenAI の 3 項目を保存し、英文 1 文で「文法を学ぶ」を押すと 4 セクション（訳文・直訳・文法ポイント・単語）が出る。
- 直訳が日本語で、` / ` 区切りになっている。
- Azure OpenAI 未設定ではボタンが出ない。
- 誤ったキーで 401、誤ったデプロイ名で 404 のメッセージが出て、訳文は残る。
- 1000 文字を超える選択で上限のメッセージが出る。
- 長い解説でもポップアップが画面からはみ出さず、中をスクロールできる。
- ツールバーのポップアップでも同じように動く。
- 解説の表示中にページをクリックするか Esc を押すと、ポップアップが閉じる。

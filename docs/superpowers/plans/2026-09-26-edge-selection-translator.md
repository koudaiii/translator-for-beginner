# Edge 選択範囲 日本語翻訳拡張 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Edge で選択したテキストを Azure Translator で日本語に翻訳し、選択範囲の下に表示する拡張機能を作る。

**Architecture:** MV3 拡張。service worker（`background.js`）が右クリックメニューとショートカットを受けて `content.js` を注入し、`content.js` からの翻訳依頼を受けて Azure API を呼ぶ。`content.js` は選択取得と Shadow DOM ポップアップ表示だけを担当する。キーは設定ページから `chrome.storage.local` に保存し、service worker 内でのみ使う。

**Tech Stack:** Manifest V3、素の JavaScript、Azure Translator Text Translation REST API v3.0。ビルド工程なし。

**Spec:** `docs/superpowers/specs/2026-09-26-edge-selection-translator-design.md`

**状態:** 全タスク実装済み（v0.3.0）。ファイルの最終内容はリポジトリ内の各ファイルが正であり、この計画書にはコードを重複して載せない。

## Global Constraints

- Manifest V3。権限は `contextMenus`, `activeTab`, `scripting`, `storage` のみ
- `host_permissions` は `https://api.cognitive.microsofttranslator.com/*` のみ
- 翻訳先は `ja`。原文言語は Azure の自動判定に任せる
- ショートカット既定は `Alt+T`、コマンド名は `translate-selection`
- 翻訳の上限は5000文字（`content.js` の `MAX_CHARS`）
- キーは `chrome.storage.local` のみに保存し、`content.js` に渡さない
- ポップアップは Shadow DOM 内。表示には `textContent` を使う
- 作業ディレクトリは git リポジトリではないため、コミット手順は含まない

## Review Focus

- 選択が空: 「テキストが選択されていません」と出る
- 5000文字超の選択: 翻訳せず、文字数と上限が出る
- 選択が日本語: 「すでに日本語です」と出る
- キー未設定: 設定ページでの登録を促すメッセージが出る
- キーが誤り（401）や無料枠超過（403/429）: HTTP ステータスを含むメッセージが出る
- 通信失敗: 例外で止まらず、メッセージが出る
- 続けて2回実行: 前回のポップアップが残らず1つだけ表示される
- 選択テキストに HTML 文字列が含まれる: 実行されず文字として表示される
- 画面右端や下端の選択: ポップアップが画面外にはみ出さない
- `chrome://` など注入不可のページ: service worker がエラーで止まらない
- 拡張機能以外からのメッセージ: `background.js` が応答しない

---

### Task 1: 拡張の骨組みと注入

**Files:**
- Create: `manifest.json`, `background.js`（メニュー、ショートカット、注入）

**Interfaces:**
- Produces: `content.js` は注入されると即座に実行される自己完結スクリプト。

- [x] **Step 1:** `manifest.json` を作る（MV3、コマンド、`web_accessible_resources` に `popup.css`）
- [x] **Step 2:** `background.js` に右クリックメニュー登録、`onClicked`、`onCommand`、`executeScript` による注入を実装する
- [x] **Step 3:** Edge に「展開して読み込み」し、メニューと `Alt+T` の両方で注入されることを確認する。`chrome://extensions` では service worker のコンソールに警告が出るだけであること

### Task 2: ポップアップ表示

**Files:**
- Create: `popup.css`, `content.js`

**Interfaces:**
- Produces: `content.js` は選択取得、文字数チェック、`chrome.runtime.sendMessage({type: 'translate', text})` の送信、応答の表示を行う。応答は `{ok: true, text, detected}` または `{ok: false, error}`。

- [x] **Step 1:** `popup.css` を作る
- [x] **Step 2:** `content.js` に Shadow DOM ポップアップ、配置の画面内補正、外側クリックと Esc での閉じる処理、空選択と文字数超過のメッセージを実装する
- [x] **Step 3:** 応答を受けて表示する。`detected === 'ja'` なら「すでに日本語です」

### Task 3: Azure 呼び出しと設定ページ

**Files:**
- Modify: `manifest.json`（`storage` 権限、`host_permissions`、`options_ui`、version 0.3.0）
- Modify: `background.js`（`translate` メッセージ処理）
- Create: `options.html`, `options.js`

**Interfaces:**
- Consumes: Task 2 のメッセージ形式 `{type: 'translate', text}`
- Produces: 応答 `{ok: true, text, detected}` または `{ok: false, error}`。ストレージのキーは `key` と `region`。

- [x] **Step 1:** 設定ページでキーとリージョンを `chrome.storage.local` に保存・読み込みする
- [x] **Step 2:** `background.js` で `POST .../translate?api-version=3.0&to=ja` を呼ぶ。キー未設定、HTTP エラー、通信失敗はそれぞれ `{ok: false, error}` を返す。リージョンが空ならヘッダを付けない
- [x] **Step 3:** `sender.id === chrome.runtime.id` を確認してから応答する
- [x] **Step 4:** 手動確認: 設定保存、英文の翻訳、日本語文、未選択、キー未設定、続けて2回実行

## 経緯の記録

内蔵 Translator API を使う初期案（Task 2 の旧版）は、Edge 154 で `Translator.create()` が全言語ペアで `NotSupportedError` になり、公式デモでも失敗したため廃止した。`LanguageDetector` / `Translator` を呼ぶコードは残していない。

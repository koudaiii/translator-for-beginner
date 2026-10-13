# Edge 選択範囲 日本語翻訳拡張 設計

## 目的
Edge のページ上で選択したテキストを、Azure Translator で日本語に翻訳し、選択範囲の近くに表示する。利用者は作者本人のみ。

## 経緯
当初は Edge 内蔵の Translator API（端末内翻訳）を使う設計だった。Edge 154（macOS 27, arm64）では `availability()` が `available` を返す一方、`Translator.create()` が全言語ペアで `NotSupportedError` になり、公式デモでも失敗した。そのため、内蔵 API をやめて Azure Translator に切り替えた。

## 前提
- Manifest V3。ストア公開はせず、「パッケージ化されていない拡張機能を読み込む」で利用する。
- 翻訳は Azure Translator の Text Translation REST API（v3.0）。価格レベルは Free F0（月200万文字まで無料）。
- 原文言語は Azure の自動判定に任せる。
- 選択テキストは Microsoft のサーバーに送信される。

## 構成
- `manifest.json`: 権限は `contextMenus`, `activeTab`, `scripting`, `storage`。`host_permissions` は `https://api.cognitive.microsofttranslator.com/*`。`options_ui` で設定ページを登録。コマンド `translate-selection`（既定 `Alt+T`）を定義。
- `background.js`: 右クリックメニューとショートカットを受け、アクティブタブへ `content.js` を注入する。`content.js` からの `translate` メッセージを受け、Azure API を呼んで結果を返す。キーは service worker 内だけで使う。
- `content.js`: 選択テキストの取得、文字数チェック、`background.js` への翻訳依頼、Shadow DOM ポップアップ表示。
- `popup.css`: ポップアップのスタイル。
- `options.html` / `options.js`: Azure のキーとリージョンを `chrome.storage.local` に保存する設定ページ。

## 動作
1. メニューまたはショートカットで `content.js` を注入する。
2. 選択が空なら「テキストが選択されていません」と表示する。
3. 5000文字を超える場合は、無料枠の保護のため翻訳せず、文字数と上限を表示する。
4. `chrome.runtime.sendMessage({type: 'translate', text})` で `background.js` に依頼する。
5. `background.js` は `POST https://api.cognitive.microsofttranslator.com/translate?api-version=3.0&to=ja` を呼ぶ。ヘッダは `Ocp-Apim-Subscription-Key`、リージョンが設定されていれば `Ocp-Apim-Subscription-Region`。
6. 応答の `detectedLanguage.language` が `ja` なら「すでに日本語です」と表示する。それ以外は訳文を選択範囲の直下に表示する。
7. キー未設定、HTTP エラー、通信失敗の場合は、その内容をポップアップに表示する。
8. ページのクリックまたは Esc でポップアップを閉じる。
9. メッセージの送信元が自分の拡張機能でなければ、`background.js` は応答しない。

## セキュリティ
- キーは `chrome.storage.local` に保存し、`content.js` やページには渡さない。同期ストレージは使わない。
- 訳文と原文は `textContent` で表示し、HTML として解釈させない。
- キーはチャットやコードに書かない。漏れた場合は Azure ポータルで再生成する。

## 対象外
翻訳履歴、内蔵 Translator API へのフォールバック、複数言語への翻訳、ストア公開の整備。

## テスト（手動）
- 設定ページでキーを保存し、英語ページで選択して日本語訳が出る。
- 右クリックメニューとショートカットの両方で動く。
- 日本語ページ、未選択、5000文字超、キー未設定で適切なメッセージが出る。
- 誤ったキーで Azure のエラー（401）がポップアップに出る。
- 続けて2回実行しても、ポップアップが1つだけ表示される。

## 備考
作業ディレクトリは git リポジトリではないため、設計書はコミットしていない。

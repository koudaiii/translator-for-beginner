const input = document.getElementById('input');
const result = document.getElementById('result');

async function translate() {
  const text = input.value.trim();
  if (!text) {
    result.textContent = 'テキストを入力してください';
    return;
  }

  result.textContent = '翻訳中…';
  try {
    const res = await chrome.runtime.sendMessage({ type: 'translate', text });
    if (!res || !res.ok) {
      result.textContent = res && res.error ? res.error : '翻訳に失敗しました';
    } else if (res.detected === 'ja') {
      result.textContent = 'すでに日本語です';
    } else {
      result.textContent = res.text;
    }
  } catch (e) {
    result.textContent = `翻訳に失敗しました: ${e.message}`;
  }
}

document.getElementById('translate').addEventListener('click', translate);

async function fillSelection() {
  try {
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
    if (!tab || tab.id === undefined) return;
    const [{ result: selected } = {}] = await chrome.scripting.executeScript({
      target: { tabId: tab.id },
      func: () => window.getSelection().toString(),
    });
    if (selected && selected.trim()) {
      input.value = selected.trim();
      await translate();
    }
  } catch (e) {
    // 選択を取得できないページ（chrome:// など）では何もしない
  }
}

fillSelection();

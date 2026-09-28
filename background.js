const MENU_ID = 'translate-selection';
const ENDPOINT = 'https://api.cognitive.microsofttranslator.com/translate?api-version=3.0&to=ja';

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: MENU_ID,
    title: '選択範囲を日本語に翻訳',
    contexts: ['selection'],
  });
});

async function run(tab) {
  if (!tab || tab.id === undefined) return;
  try {
    await chrome.scripting.executeScript({
      target: { tabId: tab.id },
      files: ['content.js'],
    });
  } catch (e) {
    console.warn('このページには注入できません:', e.message);
  }
}

chrome.contextMenus.onClicked.addListener((info, tab) => {
  if (info.menuItemId === MENU_ID) run(tab);
});

chrome.commands.onCommand.addListener((command, tab) => {
  if (command === MENU_ID) run(tab);
});

async function translate(text) {
  const { key, region } = await chrome.storage.local.get(['key', 'region']);
  if (!key) return { ok: false, error: '設定ページで Azure Translator のキーを登録してください' };

  const headers = {
    'Content-Type': 'application/json',
    'Ocp-Apim-Subscription-Key': key,
  };
  if (region) headers['Ocp-Apim-Subscription-Region'] = region;

  const res = await fetch(ENDPOINT, {
    method: 'POST',
    headers,
    body: JSON.stringify([{ Text: text }]),
  });
  if (!res.ok) {
    const detail = await res.text().catch(() => '');
    return { ok: false, error: `Azure がエラーを返しました (${res.status}) ${detail}`.trim() };
  }
  const [item] = await res.json();
  return {
    ok: true,
    text: item.translations[0].text,
    detected: item.detectedLanguage ? item.detectedLanguage.language : null,
  };
}

chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  if (msg && msg.type === 'translate' && sender.id === chrome.runtime.id) {
    translate(msg.text)
      .then(sendResponse)
      .catch((e) => sendResponse({ ok: false, error: `通信に失敗しました: ${e.message}` }));
    return true;
  }
});

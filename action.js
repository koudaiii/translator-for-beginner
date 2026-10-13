const input = document.getElementById('input');
const result = document.getElementById('result');

async function translate() {
  const text = input.value.trim();
  result.replaceChildren();
  if (!text) {
    result.textContent = 'テキストを入力してください';
    return;
  }

  const view = Lesson.createLessonView(result);
  try {
    await Lesson.startLesson(text, (msg) => chrome.runtime.sendMessage(msg), view);
  } catch (e) {
    result.textContent = `表示に失敗しました: ${e.message}`;
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

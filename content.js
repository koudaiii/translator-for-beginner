(async () => {
  const HOST_ID = '__jp-translate-host__';
  const MAX_CHARS = 5000;

  document.getElementById(HOST_ID)?.remove();

  const selection = window.getSelection();
  const text = selection ? selection.toString().trim() : '';
  const rect = selection && selection.rangeCount
    ? selection.getRangeAt(0).getBoundingClientRect()
    : null;

  const host = document.createElement('div');
  host.id = HOST_ID;
  const shadow = host.attachShadow({ mode: 'closed' });
  const style = document.createElement('link');
  style.rel = 'stylesheet';
  style.href = chrome.runtime.getURL('popup.css');
  const box = document.createElement('div');
  box.className = 'box';
  shadow.append(style, box);
  document.documentElement.append(host);

  function place() {
    const width = box.offsetWidth;
    const height = box.offsetHeight;
    const baseLeft = rect ? rect.left : 16;
    const baseTop = rect ? rect.bottom + 8 : 16;
    let left = baseLeft + window.scrollX;
    let top = baseTop + window.scrollY;
    const maxLeft = window.scrollX + document.documentElement.clientWidth - width - 8;
    const maxTop = window.scrollY + document.documentElement.clientHeight - height - 8;
    left = Math.max(window.scrollX + 8, Math.min(left, maxLeft));
    if (top > maxTop && rect) top = rect.top + window.scrollY - height - 8;
    top = Math.max(window.scrollY + 8, top);
    box.style.left = `${left}px`;
    box.style.top = `${top}px`;
  }

  function show(message) {
    box.textContent = message;
    place();
  }

  function close() {
    host.remove();
    document.removeEventListener('mousedown', onOutside, true);
    document.removeEventListener('keydown', onKey, true);
  }
  function onOutside(e) {
    if (!e.composedPath().includes(host)) close();
  }
  function onKey(e) {
    if (e.key === 'Escape') close();
  }
  document.addEventListener('mousedown', onOutside, true);
  document.addEventListener('keydown', onKey, true);

  if (!text) {
    show('テキストが選択されていません');
    return;
  }
  if (text.length > MAX_CHARS) {
    show(`選択範囲が長すぎます (${text.length}文字、上限${MAX_CHARS}文字)`);
    return;
  }

  const view = Lesson.createLessonView(box, place);
  place();
  try {
    await Lesson.startLesson(text, (msg) => chrome.runtime.sendMessage(msg), view);
  } catch (e) {
    show(`表示に失敗しました: ${e.message}`);
  }
})();

// 翻訳と AI 処理を並行して依頼し、届いた順に描画する。
// content.js（ページへの注入）と action.js（ツールバーのポップアップ）で共有する。
(() => {
  const isNode = typeof module !== 'undefined' && module.exports;
  if (!isNode && globalThis.Lesson) return;

  const AI_TASKS = ['reading', 'grammar', 'corrections', 'variants'];
  const PLACEHOLDERS = {
    reading: '直訳を作成中…',
    grammar: '文法を解説中…',
    corrections: '確認中…',
    variants: '作成中…',
  };
  const VARIANT_LABELS = [
    ['chat', 'チャット'],
    ['email', 'メール'],
    ['spoken', '対面・通話'],
    ['github', 'GitHub'],
  ];

  function hasKana(text) {
    return /[぀-ヿ]/.test(text);
  }

  function normalizeResult(res, fallback) {
    return res && typeof res === 'object' && 'ok' in res ? res : { ok: false, error: fallback };
  }

  async function startLesson(text, send, ui) {
    const jobs = [
      send({ type: 'translate', text }).then(
        (res) => ui.translation(normalizeResult(res, '翻訳に失敗しました')),
        (e) => ui.translation({ ok: false, error: `翻訳に失敗しました: ${e.message}` }),
      ),
    ];
    if (!hasKana(text)) {
      const available = await send({ type: 'aiAvailable' }).catch(() => false);
      if (available === true) {
        ui.aiStarted();
        for (const task of AI_TASKS) {
          jobs.push(send({ type: task, text }).then(
            (res) => ui.ai(task, normalizeResult(res, '結果を受け取れませんでした')),
            (e) => ui.ai(task, { ok: false, error: `通信に失敗しました: ${e.message}` }),
          ));
        }
      }
    }
    await Promise.all(jobs);
  }

  function el(tag, className, text) {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (text !== undefined) node.textContent = text;
    return node;
  }

  function renderTranslation(res) {
    if (!res.ok) return el('div', 'error', res.error || '翻訳に失敗しました');
    if (res.detected === 'ja') return el('div', null, 'すでに日本語です');
    return el('div', 'translation', res.text);
  }

  function renderReading(data) {
    const frag = document.createDocumentFragment();
    frag.append(el('div', 'label', '直訳'), el('div', 'literal', data.literal));
    if (data.vocabulary.length) {
      const list = el('ul', 'vocab');
      for (const v of data.vocabulary) {
        const item = el('li');
        item.append(el('span', 'term', v.term), ` /${v.ipa}/ ${v.katakana} — ${v.meaning}`);
        list.append(item);
      }
      frag.append(el('div', 'label', '単語・熟語'), list);
    }
    return frag;
  }

  function renderGrammar(data) {
    const frag = document.createDocumentFragment();
    frag.append(el('div', 'label', '文法ポイント'));
    for (const g of data.grammar) {
      const item = el('div', 'point');
      item.append(el('div', 'point-title', g.point), el('div', null, g.explanation));
      for (const ex of g.examples) {
        const example = el('div', 'example');
        example.append(el('div', 'en', ex.en), el('div', 'ja', ex.ja));
        item.append(example);
      }
      frag.append(item);
    }
    return frag;
  }

  function renderCorrections(data) {
    const frag = document.createDocumentFragment();
    frag.append(el('div', 'label', '直した箇所'));
    if (!data.corrections.length) {
      frag.append(el('div', null, '文法の誤りは見つかりませんでした'));
      return frag;
    }
    for (const c of data.corrections) {
      const line = el('div');
      line.append(el('span', 'before', c.before), ' → ', el('span', 'after', c.after));
      const item = el('div', 'correction');
      item.append(line, el('div', 'reason', c.reason));
      frag.append(item);
    }
    return frag;
  }

  function copyButton(text) {
    const button = el('button', 'copy', 'コピー');
    button.type = 'button';
    button.addEventListener('click', () => {
      Promise.resolve().then(() => navigator.clipboard.writeText(text)).then(
        () => {
          button.textContent = 'コピーしました';
          setTimeout(() => { button.textContent = 'コピー'; }, 1500);
        },
        () => { button.textContent = 'コピーできませんでした'; },
      );
    });
    return button;
  }

  function renderVariants(data) {
    const frag = document.createDocumentFragment();
    frag.append(el('div', 'label', '言い回し'));
    for (const [key, label] of VARIANT_LABELS) {
      const head = el('div', 'variant-head');
      head.append(el('span', 'variant-label', label), copyButton(data[key]));
      const item = el('div', 'variant');
      item.append(head, el('div', 'en', data[key]));
      frag.append(item);
    }
    return frag;
  }

  const RENDERERS = {
    reading: renderReading,
    grammar: renderGrammar,
    corrections: renderCorrections,
    variants: renderVariants,
  };

  function createLessonView(container, onChange = () => {}) {
    // 実行ごとに専用の root を持つ。container を空にされた古い実行は、切り離された root を更新するだけになる。
    const root = el('div', 'lesson');
    const translation = el('div', 'section', '翻訳中…');
    root.append(translation);
    container.append(root);
    const blocks = {};

    function update(node, child) {
      node.replaceChildren(child);
      onChange();
    }

    return {
      translation(res) {
        update(translation, renderTranslation(res));
      },
      aiStarted() {
        const explain = el('div', 'section');
        explain.append(el('div', 'heading', '文法解説'));
        const fix = el('div', 'section');
        fix.append(el('div', 'heading', '英文を直す'));
        for (const task of AI_TASKS) {
          blocks[task] = el('div', 'block', PLACEHOLDERS[task]);
          (task === 'reading' || task === 'grammar' ? explain : fix).append(blocks[task]);
        }
        root.append(explain, fix);
        onChange();
      },
      ai(task, res) {
        let child;
        try {
          child = res.ok
            ? RENDERERS[task](res.data)
            : el('div', 'error', res.error || '結果を読み取れませんでした');
        } catch {
          child = el('div', 'error', '結果を読み取れませんでした');
        }
        update(blocks[task], child);
      },
    };
  }

  const api = { AI_TASKS, hasKana, normalizeResult, startLesson, createLessonView };
  if (isNode) module.exports = api;
  else globalThis.Lesson = api;
})();

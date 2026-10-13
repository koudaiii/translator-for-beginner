// lesson.js の描画を Node で確かめるための最小限の DOM。
class FakeText {
  constructor(text) { this.text = text; this.parent = null; }
  get textContent() { return this.text; }
}

class FakeElement {
  constructor(tagName) {
    this.tagName = tagName;
    this.className = '';
    this.children = [];
    this.parent = null;
    this.ownText = '';
    this.listeners = {};
  }
  append(...items) {
    for (const item of items) {
      if (item instanceof FakeFragment) {
        for (const child of [...item.children]) this.adopt(child);
        item.children = [];
      } else {
        this.adopt(typeof item === 'string' ? new FakeText(item) : item);
      }
    }
  }
  adopt(child) {
    if (child.parent) child.parent.children = child.parent.children.filter((c) => c !== child);
    child.parent = this;
    this.children.push(child);
  }
  replaceChildren(...items) {
    for (const child of this.children) child.parent = null;
    this.children = [];
    this.ownText = '';
    this.append(...items);
  }
  set textContent(value) {
    this.replaceChildren();
    this.ownText = String(value);
  }
  get textContent() {
    return this.ownText + this.children.map((c) => c.textContent).join('');
  }
  addEventListener(type, listener) { this.listeners[type] = listener; }
  click() { this.listeners.click?.(); }
  findAll(className) {
    const found = [];
    for (const child of this.children) {
      if (!(child instanceof FakeElement)) continue;
      if (child.className.split(' ').includes(className)) found.push(child);
      found.push(...child.findAll(className));
    }
    return found;
  }
}

class FakeFragment extends FakeElement {}

export function installFakeDom() {
  globalThis.document = {
    createElement: (tag) => new FakeElement(tag),
    createDocumentFragment: () => new FakeFragment('#fragment'),
  };
  return new FakeElement('div');
}

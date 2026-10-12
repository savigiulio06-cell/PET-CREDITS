// Small reusable speech bubble. Text is only ever written with textContent.
export interface BubbleView {
  root: HTMLElement;
  /** Shows `text` for `ms`, then fades out. `lang` (BCP 47) is set on the element when given. */
  show(text: string, lang?: string, ms?: number): void;
  hide(): void;
}

export function createBubble(): BubbleView {
  const root = document.createElement('div');
  root.className = 'bubble';
  root.setAttribute('role', 'status');
  root.setAttribute('aria-live', 'polite');
  const body = document.createElement('span');
  body.className = 'bubble-text';
  root.append(body);
  let timer = 0;

  const clear = (): void => {
    if (timer) window.clearTimeout(timer);
    timer = 0;
  };

  return {
    root,
    show(text, lang, ms = 4500) {
      clear();
      if (lang) root.setAttribute('lang', lang);
      else root.removeAttribute('lang');
      body.textContent = text;
      root.classList.add('visible');
      timer = window.setTimeout(() => {
        timer = 0;
        root.classList.remove('visible');
      }, ms);
    },
    hide() {
      clear();
      root.classList.remove('visible');
    },
  };
}

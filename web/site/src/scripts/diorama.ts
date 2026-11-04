import { createAstroMotionLifecycle } from '@cachix/site-kit/motion';
import { animationPauser } from './motion';

interface DioramaOptions {
  interval?: number | ((state: string) => number);
  onShow?: (state: string, root: HTMLElement) => void;
}

export function diorama(selector: string, { interval = 3600, onShow }: DioramaOptions = {}) {
  for (const root of document.querySelectorAll<HTMLElement>(selector)) {
    const states = (root.dataset.states ?? '').split(' ').filter(Boolean);
    if (states.length === 0) continue;
    const targets = root.querySelectorAll<HTMLElement>('[data-state-target]');
    const panels = root.querySelectorAll<HTMLElement>('[data-panel]');
    const delay = (state: string) => typeof interval === 'function' ? interval(state) : interval;
    let index = Math.max(0, states.indexOf(root.dataset.state ?? ''));
    let pinned = false;
    let timer: number | undefined;

    const show = (state: string) => {
      index = states.indexOf(state);
      root.dataset.state = state;
      for (const target of targets) {
        target.setAttribute('aria-pressed', String(target.dataset.stateTarget === state));
      }
      for (const panel of panels) {
        panel.toggleAttribute('data-active', (panel.dataset.panel ?? '').split(' ').includes(state));
      }
      onShow?.(state, root);
    };

    const stop = () => {
      window.clearTimeout(timer);
      timer = undefined;
    };

    const schedule = () => {
      stop();
      timer = window.setTimeout(() => {
        if (!pinned) show(states[(index + 1) % states.length]);
        schedule();
      }, delay(states[index]));
    };

    for (const target of targets) {
      const state = target.dataset.stateTarget;
      if (!state || !states.includes(state)) continue;
      target.addEventListener('click', () => {
        pinned = true;
        if (root.dataset.state !== state) show(state);
      });
    }

    const pause = animationPauser(root);
    show(states[index]);
    createAstroMotionLifecycle({
      target: root,
      threshold: 0.25,
      onChange: (active) => {
        pause(active);
        if (active) schedule();
        else stop();
      },
    });
  }
}

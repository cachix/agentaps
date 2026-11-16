import { createMotionLifecycle } from '@cachix/site-kit/motion';
import { animationPauser, onPageLoad } from './motion';

interface DioramaOptions {
  interval?: number | ((state: string) => number);
  onShow?: (state: string, root: HTMLElement, active: boolean) => void | (() => void);
}

export function diorama(selector: string, { interval = 3600, onShow }: DioramaOptions = {}) {
  onPageLoad(() => {
    const cleanups: (() => void)[] = [];
    for (const root of document.querySelectorAll<HTMLElement>(selector)) {
      const states = (root.dataset.states ?? '').split(' ').filter(Boolean);
      if (states.length === 0) continue;
      const targets = root.querySelectorAll<HTMLElement>('[data-state-target]');
      const panels = root.querySelectorAll<HTMLElement>('[data-panel]');
      const delay = (state: string) => typeof interval === 'function' ? interval(state) : interval;
      const events = new AbortController();
      const pause = animationPauser(root);
      let index = Math.max(0, states.indexOf(root.dataset.state ?? ''));
      let pinned = false;
      let active = false;
      let timer: number | undefined;
      let cleanupScene: void | (() => void);

      const show = (state: string) => {
        cleanupScene?.();
        index = states.indexOf(state);
        root.dataset.state = state;
        for (const target of targets) {
          target.setAttribute('aria-pressed', String(target.dataset.stateTarget === state));
        }
        for (const panel of panels) {
          panel.toggleAttribute('data-active', (panel.dataset.panel ?? '').split(' ').includes(state));
        }
        cleanupScene = onShow?.(state, root, active);
        pause(active);
      };

      const stop = () => {
        window.clearTimeout(timer);
        timer = undefined;
      };

      const schedule = () => {
        stop();
        if (!active || pinned) return;
        timer = window.setTimeout(() => {
          show(states[(index + 1) % states.length]);
          schedule();
        }, delay(states[index]));
      };

      for (const target of targets) {
        const state = target.dataset.stateTarget;
        if (!state || !states.includes(state)) continue;
        const select = () => {
          pinned = true;
          stop();
          if (root.dataset.state !== state) show(state);
        };
        target.addEventListener('click', select, { signal: events.signal });
        if (target.getAttribute('role') === 'button') {
          target.addEventListener('keydown', (event) => {
            if (event.key !== 'Enter' && event.key !== ' ') return;
            event.preventDefault();
            select();
          }, { signal: events.signal });
        }
      }

      const motion = createMotionLifecycle({
        target: root,
        threshold: 0.25,
        onChange: (isActive) => {
          active = isActive;
          show(states[index]);
          schedule();
        },
      });
      cleanups.push(() => {
        motion.destroy();
        stop();
        cleanupScene?.();
        events.abort();
      });
    }
    return () => cleanups.forEach((cleanup) => cleanup());
  });
}

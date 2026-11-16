export function animationPauser(target: HTMLElement) {
  let paused: Animation[] = [];
  return (active: boolean) => {
    if (active) {
      for (const animation of paused) {
        if (animation.playState === 'paused') animation.play();
      }
      paused = [];
      return;
    }
    paused = [...new Set([
      ...paused.filter((animation) => animation.playState === 'paused'),
      ...target.getAnimations({ subtree: true }).filter((animation) => animation.playState === 'running'),
    ])];
    for (const animation of paused) animation.pause();
  };
}

export function onPageLoad(setup: () => void | (() => void)) {
  let cleanup: void | (() => void);
  document.addEventListener('astro:page-load', () => {
    cleanup?.();
    cleanup = setup();
  });
  document.addEventListener('astro:before-swap', () => {
    cleanup?.();
    cleanup = undefined;
  });
}

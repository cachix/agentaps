export function animationPauser(target: HTMLElement) {
  let paused: Animation[] = [];
  return (active: boolean) => {
    if (active) {
      for (const animation of paused) animation.play();
      paused = [];
      return;
    }
    paused = target.getAnimations({ subtree: true }).filter((animation) => animation.playState === 'running');
    for (const animation of paused) animation.pause();
  };
}

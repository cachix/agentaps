export function pinChatsToLatest(root: ParentNode = document) {
  const observer = new ResizeObserver((entries) => {
    for (const { target } of entries) {
      if (target instanceof HTMLElement && target.matches('.app-pane')) {
        const unit = parseFloat(getComputedStyle(target).getPropertyValue('--u'));
        target.toggleAttribute('data-compact', target.clientWidth / unit < 600);
      }
      const chat = target.closest('.app-chat');
      if (chat) chat.scrollTop = chat.scrollHeight;
    }
  });
  for (const pane of root.querySelectorAll('.app-pane')) observer.observe(pane);
  for (const chat of root.querySelectorAll('.app-chat')) {
    observer.observe(chat);
    for (const column of chat.children) observer.observe(column);
  }
  return () => observer.disconnect();
}

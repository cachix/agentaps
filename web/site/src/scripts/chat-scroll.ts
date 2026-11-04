export function pinChatsToLatest(root: ParentNode = document) {
  const observer = new ResizeObserver((entries) => {
    for (const { target } of entries) {
      const chat = target.closest('.app-chat');
      if (chat) chat.scrollTop = chat.scrollHeight;
    }
  });
  for (const chat of root.querySelectorAll('.app-chat')) {
    observer.observe(chat);
    for (const column of chat.children) observer.observe(column);
  }
}

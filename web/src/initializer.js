// Trunk reports bytes from the app download without a second fetch.
export default function initializer() {
  const progress = detail => window.dispatchEvent(new CustomEvent('AgentapsLoadProgress', { detail }));
  return {
    onStart: () => progress({ current: 0, total: 0 }),
    onProgress: progress,
    // The Rust entry point starts graphics asynchronously. Keep the loading UI
    // until it signals AgentapsReady, even after Trunk finishes initialization.
    onSuccess: () => progress({ starting: true }),
    onFailure: () => window.dispatchEvent(new Event('AgentapsStartupFailed')),
  };
}

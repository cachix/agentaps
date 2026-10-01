const releaseUrl = 'https://api.github.com/repos/domenkozar/agentaps/releases/latest';
const patterns = {
  linux: /(?:linux|unknown-linux|appimage|\.deb$|\.rpm$)/i,
  macos: /(?:macos-aarch64|_aarch64)\.dmg$/i,
  windows: /(?:windows|pc-windows|\.msi$|\.exe$)/i,
};
const platformNames = { linux: 'Linux', macos: 'macOS (Apple Silicon)', windows: 'Windows' };
const installerFile = /\.(?:AppImage|deb|rpm|tar\.gz|tar\.xz|zip|dmg|pkg|msi|exe)$/i;
const downloadLabel = (platform, asset) => {
  if (platform === 'linux' && /\.deb$/i.test(asset.name)) return 'Linux DEB';
  if (platform === 'linux' && /\.AppImage$/i.test(asset.name)) return 'Linux AppImage';
  return platformNames[platform];
};

const userAgent = navigator.userAgent || '';
const isMobile = navigator.userAgentData?.mobile
  || /Android|iPhone|iPad|iPod/i.test(userAgent)
  || (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1);
if (!isMobile) {
  const platform = navigator.userAgentData?.platform || navigator.platform || userAgent;
  const currentPlatform = /Win/i.test(platform) ? 'windows'
    : /Mac/i.test(platform) ? 'macos'
      : /Linux/i.test(platform) ? 'linux' : null;
  if (currentPlatform) {
    document.querySelector(`.platform-download[data-platform="${currentPlatform}"]`)?.classList.add('is-current-platform');
  }
}

fetch(releaseUrl, { headers: { Accept: 'application/vnd.github+json' } })
  .then((response) => response.ok ? response.json() : null)
  .then((release) => {
    if (!release || !Array.isArray(release.assets)) return;
    let publishedPlatforms = 0;
    for (const [platform, pattern] of Object.entries(patterns)) {
      const slot = document.querySelector(`.platform-download[data-platform="${platform}"]`);
      if (!slot) continue;
      const assets = release.assets.filter((asset) => pattern.test(asset.name) && installerFile.test(asset.name) && asset.browser_download_url);
      if (assets.length === 0) continue;
      publishedPlatforms += 1;
      slot.replaceChildren(...assets.map((asset) => {
        const link = document.createElement('a');
        link.className = 'platform-link';
        link.href = asset.browser_download_url;
        link.textContent = downloadLabel(platform, asset);
        return link;
      }));
    }
    if (publishedPlatforms === Object.keys(patterns).length && release.tag_name && release.html_url) {
      const version = document.querySelector('#release-version');
      if (version) {
        version.textContent = `Agentaps ${release.tag_name.replace(/^v/, '')}`;
        version.href = release.html_url;
      }
      const downloadLabel = document.querySelector('#download-label');
      if (downloadLabel) {
        downloadLabel.textContent = `Get Agentaps ${release.tag_name.replace(/^v/, '')}`;
      }
    }
  })
  .catch(() => {});

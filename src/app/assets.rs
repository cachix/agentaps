use crate::config::KnownAgent;
use gpui_kit::assets::Assets;
use gpui_kit::component::Icon;
use gpui_kit::{AssetSource, Result, SharedString};
use std::borrow::Cow;

pub(super) struct AppAssets;

gpui_kit::assets::icon_assets!(SettingsAssets, [Keyboard]);

const APP_ASSETS: [(&str, &[u8]); 6] = [
    ("icons/brain.svg", include_bytes!("../../assets/brain.svg")),
    (
        "icons/mobile.svg",
        include_bytes!("../../assets/mobile.svg"),
    ),
    (
        "icons/agents/codex.svg",
        include_bytes!("../../assets/agents/codex.svg"),
    ),
    (
        "icons/agents/claude.svg",
        include_bytes!("../../assets/agents/claude.svg"),
    ),
    (
        "icons/agents/gemini.svg",
        include_bytes!("../../assets/agents/gemini.svg"),
    ),
    (
        "icons/agents/opencode.svg",
        include_bytes!("../../assets/agents/opencode.svg"),
    ),
];

pub(super) fn agent_icon(agent: KnownAgent) -> Icon {
    Icon::empty().path(match agent {
        KnownAgent::Codex => "icons/agents/codex.svg",
        KnownAgent::Claude => "icons/agents/claude.svg",
        KnownAgent::GeminiCli => "icons/agents/gemini.svg",
        KnownAgent::OpenCode => "icons/agents/opencode.svg",
    })
}

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match APP_ASSETS.iter().find(|(asset, _)| *asset == path) {
            Some((_, bytes)) => Ok(Some(Cow::Borrowed(bytes))),
            None => match SettingsAssets.load(path)? {
                Some(icon) => Ok(Some(icon)),
                None => Assets.load(path),
            },
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut assets = Assets.list(path)?;
        assets.extend(SettingsAssets.list(path)?);
        assets.extend(
            APP_ASSETS
                .iter()
                .filter(|(asset, _)| asset.starts_with(path))
                .map(|(asset, _)| SharedString::from(*asset)),
        );
        Ok(assets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_key_binding_icons() {
        let listed = AppAssets.list("icons/").unwrap();
        let path = "icons/keyboard.svg";
        let svg = AppAssets
            .load(path)
            .unwrap()
            .expect("settings icon is bundled");
        assert!(svg.starts_with(b"<svg"), "{path}");
        assert!(
            listed.iter().any(|listed| listed.as_ref() == path),
            "{path}"
        );
    }

    #[test]
    fn serves_bundled_agent_logos() {
        let listed = AppAssets.list("icons/agents/").unwrap();
        for agent in ["codex", "claude", "gemini", "opencode"] {
            let path = format!("icons/agents/{agent}.svg");
            let svg = AppAssets.load(&path).unwrap().unwrap();
            assert!(svg.starts_with(b"<svg"), "{path}");
            assert!(
                listed.iter().any(|listed| listed.as_ref() == path),
                "{path}"
            );
        }
    }
}

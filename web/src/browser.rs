//! Browser storage, WebAuthn, and camera bindings.
use iroh::EndpointId;
use js_sys::{Promise, Reflect, Uint8Array};
use std::str::FromStr;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/src/browser.js")]

extern "C" {
    pub(super) fn pollDelay() -> Promise;
    pub(super) fn savedConnections() -> String;
    pub(super) fn renameSavedConnection(id: String) -> bool;
    pub(super) fn promptProjectPath() -> JsValue;
    pub(super) fn promptAcpCommand() -> JsValue;
    pub(super) fn canUsePhoneUnlock() -> bool;
    pub(super) fn clearPairingHash();
    pub(super) fn browserClientName() -> String;
    pub(super) fn pageHidden() -> bool;
    pub(super) fn stopQrCamera();
    #[wasm_bindgen(catch)]
    pub(super) async fn startQrCamera() -> Result<(), JsValue>;
    #[wasm_bindgen(catch)]
    pub(super) async fn nextQrFrame() -> Result<JsValue, JsValue>;
    pub(super) fn beginPasskeyEnrollment() -> Promise;
    pub(super) fn beginPasskeyUnlock(id: String) -> Promise;
    pub(super) fn discardPasskeyEnrollment();
    #[wasm_bindgen(catch)]
    pub(super) async fn saveConnection(
        passphrase: String,
        pairing: String,
    ) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch)]
    pub(super) async fn unlockConnection(
        passphrase: String,
        id: String,
    ) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch)]
    pub(super) async fn finishPasskeyEnrollment(pairing: String) -> Result<JsValue, JsValue>;
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Protection {
    None,
    Passphrase,
    Phone,
}

impl Protection {
    pub(super) fn from_code(code: u8) -> Self {
        match code {
            1 => Self::Passphrase,
            2 => Self::Phone,
            _ => Self::None,
        }
    }
}

#[derive(Clone, serde::Deserialize)]
pub(super) struct SavedConnection {
    pub(super) id: String,
    pub(super) label: String,
    pub(super) protection: u8,
}

pub(super) fn saved_connections() -> Vec<SavedConnection> {
    serde_json::from_str(&savedConnections()).unwrap_or_default()
}

pub(super) fn js_error(error: JsValue) -> String {
    error
        .as_string()
        .or_else(|| {
            Reflect::get(&error, &JsValue::from_str("message"))
                .ok()?
                .as_string()
        })
        .unwrap_or_else(|| "Could not unlock connection".into())
}

pub(super) fn pairing() -> Option<(EndpointId, String)> {
    let hash = web_sys::window()?.location().hash().ok()?;
    if hash.is_empty() {
        return None;
    }
    clearPairingHash();
    parse_pairing(hash.trim_start_matches('#'))
}

pub(super) fn parse_pairing(value: &str) -> Option<(EndpointId, String)> {
    let (endpoint, token) = value.split_once(':')?;
    if token.len() != 64 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some((EndpointId::from_str(endpoint).ok()?, token.into()))
}

pub(super) fn decode_qr_frame(frame: &JsValue) -> Result<Option<(EndpointId, String)>, String> {
    if frame.is_null() {
        return Ok(None);
    }
    let property = |key| Reflect::get(frame, &JsValue::from_str(key)).ok();
    let Some(width) = property("width").and_then(|value| value.as_f64()) else {
        return Ok(None);
    };
    let Some(height) = property("height").and_then(|value| value.as_f64()) else {
        return Ok(None);
    };
    let (width, height) = (width as usize, height as usize);
    if !(80..=1280).contains(&width) || !(80..=1280).contains(&height) {
        return Ok(None);
    }
    let Some(raw_pixels) = property("pixels") else {
        return Ok(None);
    };
    let pixels = Uint8Array::new(&raw_pixels).to_vec();
    if pixels.len() != width * height {
        return Ok(None);
    }
    let mut image =
        rqrr::PreparedImage::prepare_from_greyscale(width, height, |x, y| pixels[y * width + x]);
    for grid in image.detect_grids() {
        let Ok((_, content)) = grid.decode() else {
            continue;
        };
        let Ok(url) = web_sys::Url::new(&content) else {
            continue;
        };
        let Some(pairing) = parse_pairing(url.hash().trim_start_matches('#')) else {
            continue;
        };
        let origin = web_sys::window()
            .and_then(|window| window.location().origin().ok())
            .ok_or("Could not verify this site's address")?;
        if url.origin() != origin {
            return Err("This QR code points to a different site. Open the site shown in the desktop pairing link and scan again.".into());
        }
        return Ok(Some(pairing));
    }
    Ok(None)
}

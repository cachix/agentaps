use crate::config::{ChatImage, Config};
use base64::{Engine, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

const IMAGE_TYPES: [(&str, &[&str]); 4] = [
    ("image/png", &["png"]),
    ("image/jpeg", &["jpg", "jpeg"]),
    ("image/gif", &["gif"]),
    ("image/webp", &["webp"]),
];

fn attach_error(error: impl std::fmt::Display) -> String {
    format!("Could not attach image: {error}")
}

pub(crate) fn supported_mime_type(mime_type: &str) -> bool {
    IMAGE_TYPES.iter().any(|(known, _)| *known == mime_type)
}

fn extension(mime_type: &str) -> &'static str {
    IMAGE_TYPES
        .iter()
        .find(|(known, _)| *known == mime_type)
        .map_or("img", |(_, extensions)| extensions[0])
}

pub(crate) fn read_image_file(path: &Path) -> Option<Result<(String, Vec<u8>), String>> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    let (mime_type, _) = IMAGE_TYPES
        .iter()
        .find(|(_, extensions)| extensions.contains(&extension.as_str()))?;
    Some(
        fs::read(path)
            .map(|bytes| ((*mime_type).to_owned(), bytes))
            .map_err(attach_error),
    )
}

#[derive(Clone, Debug)]
pub(crate) struct ImageStore {
    dir: Arc<Path>,
}

impl ImageStore {
    pub(crate) fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into().into(),
        }
    }

    #[cfg(test)]
    pub(crate) fn for_tests() -> Self {
        Self::new(std::env::temp_dir().join(format!("agentaps-images-{}", std::process::id())))
    }

    pub(crate) fn open() -> Self {
        Self::new(
            crate::config::images_path()
                .unwrap_or_else(|_| std::env::temp_dir().join("agentaps-images")),
        )
    }

    pub(crate) fn path(&self, image: &ChatImage) -> PathBuf {
        self.dir
            .join(format!("{}.{}", image.sha256, extension(&image.mime_type)))
    }

    pub(crate) fn save(&self, mime_type: &str, bytes: &[u8]) -> Result<ChatImage, String> {
        let image = ChatImage {
            mime_type: mime_type.to_owned(),
            sha256: hex::encode(Sha256::digest(bytes)),
        };
        let path = self.path(&image);
        if path.exists() {
            return Ok(image);
        }
        fs::create_dir_all(&self.dir).map_err(attach_error)?;
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary).map_err(attach_error)?;
        file.write_all(bytes).map_err(attach_error)?;
        file.sync_all().map_err(attach_error)?;
        drop(file);
        match fs::rename(&temporary, &path) {
            Err(_) if path.exists() => {
                let _ = fs::remove_file(&temporary);
                Ok(image)
            }
            result => result.map(|()| image).map_err(attach_error),
        }
    }

    pub(crate) fn save_base64(&self, mime_type: &str, data: &str) -> Result<ChatImage, String> {
        let bytes = STANDARD.decode(data).map_err(attach_error)?;
        self.save(mime_type, &bytes)
    }

    pub(crate) fn base64(&self, image: &ChatImage) -> Result<String, String> {
        fs::read(self.path(image))
            .map(|bytes| STANDARD.encode(bytes))
            .map_err(attach_error)
    }

    pub(crate) fn remove_unused(&self, config: &Config) {
        let queued: HashSet<&str> = config
            .projects
            .iter()
            .flat_map(|project| &project.agents)
            .flat_map(|agent| &agent.pending_prompts)
            .flat_map(|prompt| &prompt.images)
            .map(|image| image.sha256.as_str())
            .collect();
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return;
        };
        for path in entries.flatten().map(|entry| entry.path()) {
            if path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .is_some_and(|stem| !queued.contains(stem))
            {
                let _ = fs::remove_file(path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AgentConfig, ProjectConfig, Prompt};

    #[test]
    fn identical_images_share_one_file() {
        let temp = tempfile::tempdir().unwrap();
        let store = ImageStore::new(temp.path());
        let first = store.save("image/png", b"pixels").unwrap();
        let second = store
            .save_base64("image/png", &STANDARD.encode(b"pixels"))
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
        assert_eq!(store.base64(&first).unwrap(), STANDARD.encode(b"pixels"));
        assert!(
            store
                .path(&first)
                .ends_with(format!("{}.png", first.sha256))
        );
    }

    #[test]
    fn startup_keeps_only_images_of_queued_prompts() {
        let temp = tempfile::tempdir().unwrap();
        let store = ImageStore::new(temp.path());
        let queued = store.save("image/jpeg", b"queued").unwrap();
        let replayed = store.save("image/png", b"replayed").unwrap();
        let mut agent: AgentConfig =
            serde_json::from_str(r#"{"id":1,"command":["agent"]}"#).unwrap();
        agent.pending_prompts.push(Prompt {
            text: "Next".into(),
            images: vec![queued.clone()],
            ..Prompt::default()
        });
        let config = Config {
            projects: vec![ProjectConfig {
                path: "/project".into(),
                ssh_host: None,
                agents: vec![agent],
            }],
            ..Config::default()
        };

        store.remove_unused(&config);
        assert!(store.path(&queued).exists());
        assert!(!store.path(&replayed).exists());
    }

    #[test]
    fn only_image_types_agents_accept_are_read() {
        let temp = tempfile::tempdir().unwrap();
        let shot = temp.path().join("shot.PNG");
        fs::write(&shot, b"png").unwrap();
        assert_eq!(
            read_image_file(&shot),
            Some(Ok(("image/png".to_owned(), b"png".to_vec())))
        );
        assert!(matches!(
            read_image_file(&temp.path().join("missing.jpeg")),
            Some(Err(error)) if error.starts_with("Could not attach image: ")
        ));
        assert_eq!(read_image_file(Path::new("drawing.bmp")), None);
        assert_eq!(read_image_file(Path::new("Makefile")), None);
        assert!(supported_mime_type("image/webp"));
        assert!(!supported_mime_type("image/svg+xml"));
    }
}

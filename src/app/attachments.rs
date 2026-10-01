use super::*;
use crate::images::{read_image_file, supported_mime_type};
use gpui_kit::{ClipboardEntry, ClipboardItem, ExternalPaths};

type ImageBytes = Result<(String, Vec<u8>), String>;

fn image_files(paths: &ExternalPaths) -> Vec<ImageBytes> {
    paths
        .paths()
        .iter()
        .filter_map(|path| read_image_file(path))
        .collect()
}

fn clipboard_images(item: &ClipboardItem) -> Vec<ImageBytes> {
    item.entries()
        .iter()
        .flat_map(|entry| match entry {
            ClipboardEntry::Image(image) if supported_mime_type(image.format.mime_type()) => {
                vec![Ok((
                    image.format.mime_type().to_owned(),
                    image.bytes.clone(),
                ))]
            }
            ClipboardEntry::ExternalPaths(paths) => image_files(paths),
            ClipboardEntry::Image(_) | ClipboardEntry::String(_) => Vec::new(),
        })
        .collect()
}

impl Workspace {
    pub(super) fn paste_images(&mut self, item: &ClipboardItem, cx: &mut Context<Self>) -> bool {
        let images = clipboard_images(item);
        if images.is_empty() {
            return false;
        }
        self.attach_images(images, cx);
        true
    }

    pub(super) fn drop_images(&mut self, paths: &ExternalPaths, cx: &mut Context<Self>) {
        let images = image_files(paths);
        if !images.is_empty() {
            self.attach_images(images, cx);
        }
    }

    pub(super) fn remove_draft_image(
        &mut self,
        agent_id: u64,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        if let Some(images) = self.conversation.draft_images.get_mut(&agent_id)
            && index < images.len()
        {
            images.remove(index);
            cx.notify();
        }
    }

    fn attach_images(&mut self, images: Vec<ImageBytes>, cx: &mut Context<Self>) {
        let Some(SessionLocation {
            project_index,
            agent_index,
        }) = self.view.displayed_session()
        else {
            return;
        };
        let agent = &self.projects[project_index].agents[agent_index];
        if agent.protocol.is_none() {
            self.notice = Some(Notice::Error(
                "Wait for the agent to connect before adding images.".into(),
            ));
        } else if !agent.accepts_images {
            self.notice = Some(Notice::Error("This agent does not accept images.".into()));
        } else {
            let agent_id = agent.config.id;
            for image in images {
                match image.and_then(|(mime_type, bytes)| self.images.save(&mime_type, &bytes)) {
                    Ok(image) => self
                        .conversation
                        .draft_images
                        .entry(agent_id)
                        .or_default()
                        .push(image),
                    Err(error) => self.notice = Some(Notice::Error(error)),
                }
            }
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{Image, ImageFormat};

    #[test]
    fn clipboard_images_come_from_image_data_and_copied_image_files() {
        let temp = tempfile::tempdir().unwrap();
        let photo = temp.path().join("photo.jpg");
        std::fs::write(&photo, b"jpeg").unwrap();
        let notes = temp.path().join("notes.txt");
        std::fs::write(&notes, b"text").unwrap();
        let item = ClipboardItem::from(ClipboardEntry::Image(Image::from_bytes(
            ImageFormat::Png,
            b"png".to_vec(),
        )));
        assert_eq!(
            clipboard_images(&item),
            [Ok(("image/png".to_owned(), b"png".to_vec()))]
        );
        let item = ClipboardItem::from(ClipboardEntry::ExternalPaths(ExternalPaths(
            [photo, notes].into_iter().collect(),
        )));
        assert_eq!(
            clipboard_images(&item),
            [Ok(("image/jpeg".to_owned(), b"jpeg".to_vec()))]
        );
        assert!(clipboard_images(&ClipboardItem::new_string("text".into())).is_empty());
        let item = ClipboardItem::from(ClipboardEntry::Image(Image::from_bytes(
            ImageFormat::Bmp,
            b"bmp".to_vec(),
        )));
        assert!(clipboard_images(&item).is_empty());
    }
}

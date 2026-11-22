use gpui_kit::component::{
    WindowExt as _,
    notification::Notification,
    text::{
        InlineElement, InlineRenderContext, MarkdownExtensions, MarkdownNode, MarkdownParseContext,
        MarkdownPlugin, TextView,
    },
    tooltip::Tooltip,
};
use gpui_kit::{
    App, ClickEvent, MouseButton, SharedString, StatefulInteractiveElement, Window, div,
    prelude::*, px, rems,
};
use markdown::{ParseOptions, mdast::Node};
use std::{cell::RefCell, collections::HashMap, sync::LazyLock};

#[cfg(test)]
use gpui_kit::base::TestSupportExt as _;

pub(super) fn link_previews() -> MarkdownExtensions {
    static EXTENSIONS: LazyLock<MarkdownExtensions> =
        LazyLock::new(|| MarkdownExtensions::default().plugin(LinkPreview));
    EXTENSIONS.clone()
}

pub(super) fn response_link_click(
    url: &SharedString,
    event: &ClickEvent,
    window: &mut Window,
    cx: &mut App,
) {
    match event {
        ClickEvent::Mouse(click) if click.up.button == MouseButton::Right => {
            cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(url.to_string()));
            window.push_notification(Notification::success("Link copied"), cx);
        }
        ClickEvent::Mouse(click)
            if matches!(click.up.button, MouseButton::Left | MouseButton::Middle) =>
        {
            cx.open_url(url);
        }
        ClickEvent::Keyboard(_) => cx.open_url(url),
        ClickEvent::Touch(click) if !click.long_press => cx.open_url(url),
        _ => {}
    }
}

struct LinkPreview;

struct Link {
    destination: SharedString,
    markdown: SharedString,
}

impl MarkdownPlugin for LinkPreview {
    fn name(&self) -> &str {
        "conversation-link"
    }

    fn parse(&self, node: &Node, cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        if !matches!(node, Node::Link(_) | Node::LinkReference(_)) {
            return None;
        }
        let source = cx.node_source(node)?;
        let (destination, markdown): (SharedString, SharedString) = match node {
            Node::Link(link) => (link.url.clone().into(), source.into()),
            Node::LinkReference(link) => {
                let destination = link_destination(cx.source(), &link.identifier)?;
                let start = node.position()?.start.offset;
                let end = link.children.last()?.position()?.end.offset;
                let label = source.get(1..end.checked_sub(start)?)?;
                let escaped = destination
                    .replace('\\', "\\\\")
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;");
                (destination, format!("[{label}](<{escaped}>)").into())
            }
            _ => return None,
        };
        if destination.is_empty() {
            return None;
        }
        Some(
            MarkdownNode::new(
                self.name().to_owned(),
                Link {
                    destination,
                    markdown,
                },
            )
            .text(
                node.to_string()
                    .replace("\r\n", " ")
                    .replace(['\r', '\n'], " "),
            )
            .markdown(source),
        )
    }

    fn render_inline(
        &self,
        node: &MarkdownNode,
        _: &InlineRenderContext,
        window: &mut Window,
        _: &mut App,
    ) -> Option<InlineElement> {
        let link = node.data::<Link>()?;
        let destination = link.destination.clone();
        // Let long labels wrap inside the inline element.
        let max_width =
            (window.viewport_size().width * 0.6).min(rems(24.).to_pixels(window.rem_size()));
        let trigger = div()
            .id("link-preview")
            .max_w(max_width)
            .tooltip(move |window, cx| {
                let destination = destination.clone();
                let width = (window.viewport_size().width - px(48.))
                    .max(px(1.))
                    .min(rems(32.).to_pixels(window.rem_size()));
                Tooltip::element(move |_, _| {
                    let text = div()
                        .id("link-destination")
                        .role(gpui_kit::accesskit::Role::Label)
                        .aria_label(destination.clone())
                        .max_w(width)
                        .whitespace_normal()
                        .child(destination.clone());
                    #[cfg(test)]
                    let text = text.test_support();
                    text
                })
                .build(window, cx)
            })
            .child(
                TextView::markdown("link-label", link.markdown.clone())
                    .selectable(false)
                    .on_link_click(response_link_click),
            );
        #[cfg(test)]
        let trigger = trigger
            .test_support()
            .aria_label(node.accessibility_name().to_owned());
        Some(InlineElement::new(trigger))
    }
}

fn link_destination(source: &str, identifier: &str) -> Option<SharedString> {
    // The extension receives one node at a time. Cache the definitions for the
    // current parse so a document with many reference links is only walked once.
    #[derive(Default)]
    struct Definitions {
        source: String,
        links: HashMap<String, SharedString>,
    }
    thread_local! {
        static DEFINITIONS: RefCell<Definitions> = RefCell::default();
    }
    fn collect(node: Node, links: &mut HashMap<String, SharedString>) {
        match node {
            Node::Definition(definition) => {
                links.insert(definition.identifier, definition.url.into());
            }
            mut node => {
                if let Some(children) = node.children_mut() {
                    for child in std::mem::take(children) {
                        collect(child, links);
                    }
                }
            }
        }
    }
    DEFINITIONS.with_borrow_mut(|definitions| {
        if definitions.source != source {
            definitions.source.clear();
            definitions.source.push_str(source);
            definitions.links.clear();
            let mut options = ParseOptions::gfm();
            options.constructs.math_text = true;
            options.constructs.math_flow = true;
            if let Ok(root) = markdown::to_mdast(source, &options) {
                collect(root, &mut definitions.links);
            }
        }
        definitions.links.get(identifier).cloned()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{
        Context, Render, TestAppContext, VisualTestContext, WindowOptions, point, size,
    };
    use std::time::Duration;

    const SKILL: &str = "agentaps://skills/agentaps/SKILL.md";

    struct Conversation(String);

    impl Render for Conversation {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().p_3().child(
                TextView::markdown("message", self.0.clone()).markdown_extensions(link_previews()),
            )
        }
    }

    #[gpui_kit::test]
    fn hovering_a_conversation_link_shows_its_full_destination(cx: &mut TestAppContext) {
        let reference = format!(
            "https://example.com/docs?q={}&format=full#details",
            "abcdef".repeat(30)
        );
        let message = format!("[Skill]({SKILL}) and [Reference][docs]\n\n[docs]: {reference}");
        let (window, _) = cx.update(|cx| {
            gpui_kit::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                cx.new(|_| Conversation(message))
            })
            .unwrap()
        });
        let cx = &mut VisualTestContext::from_window(window, cx);
        cx.simulate_resize(size(px(480.), px(260.)));
        cx.run_until_parked();
        for (label, expected) in [("Skill", SKILL), ("Reference", reference.as_str())] {
            let position = cx.update(|window, cx| {
                window.draw(cx).clear(cx);
                gpui_kit::base::test_support::snapshots(window)
                    .iter()
                    .find(|element| element.label() == Some(label))
                    .expect("visible link label")
                    .bounds()
                    .center()
            });
            cx.update(|window, cx| window.simulate_mouse_move(position, cx));
            cx.dispatcher.advance_clock(Duration::from_secs(1));
            cx.run_until_parked();
            cx.update(|window, cx| {
                window.draw(cx).clear(cx);
                let snapshots = gpui_kit::base::test_support::snapshots(window);
                let tooltip = snapshots
                    .iter()
                    .find(|element| element.label() == Some(expected) && element.visible())
                    .expect("visible full destination");
                assert!(tooltip.bounds().right() <= window.viewport_size().width);
                if label == "Reference" {
                    assert!(tooltip.bounds().size.height > px(24.));
                }
            });
            cx.simulate_mouse_down(position, MouseButton::Right, Default::default());
            cx.simulate_mouse_up(position, MouseButton::Right, Default::default());
            assert_eq!(
                cx.read_from_clipboard()
                    .and_then(|item| item.text())
                    .as_deref(),
                Some(expected)
            );
            cx.update(|window, cx| window.simulate_mouse_move(point(px(2.), px(2.)), cx));
            cx.update(|window, cx| {
                window.draw(cx).clear(cx);
                assert!(
                    !gpui_kit::base::test_support::snapshots(window)
                        .iter()
                        .any(|element| { element.label() == Some(expected) && element.visible() })
                );
            });
        }
    }
}

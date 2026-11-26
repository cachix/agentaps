use super::*;
use gpui_kit::component::diff::{DiffLinePosition, DiffLineRange, DiffSide};
use std::fmt::Write as _;

pub(super) struct DiffComment {
    range: DiffLineRange,
    source: [String; 2],
    pub(super) attached: bool,
    pub(super) input: Entity<TextareaState>,
    _subscription: Subscription,
}

impl DiffComment {
    pub(super) fn id(&self) -> u64 {
        self.input.entity_id().as_u64()
    }

    pub(super) fn position(&self) -> DiffLinePosition {
        DiffLinePosition::new(
            self.range.path().clone(),
            self.range.end_side(),
            self.range.end(),
        )
    }

    pub(super) fn location(&self) -> String {
        let range = &self.range;
        let side = |side| match side {
            DiffSide::Original => "HEAD",
            DiffSide::Modified => "checkout",
        };
        let (path, start, end) = (range.path(), range.start(), range.end());
        let (first, last) = (side(range.side()), side(range.end_side()));
        if first != last {
            format!("{path}:{start} ({first}) to {path}:{end} ({last})")
        } else if start == end {
            format!("{path}:{start} ({first})")
        } else {
            format!("{path}:{start}-{end} ({first})")
        }
    }

    pub(super) fn update_attachment(&mut self, files: &[DiffFile]) {
        self.attached =
            range_source(files, &self.range).is_some_and(|source| self.source == source);
    }
}

fn range_source<'a>(files: &'a [DiffFile], range: &DiffLineRange) -> Option<[&'a str; 2]> {
    let file = files
        .iter()
        .find(|file| file.path == range.path().as_ref())?;
    let first = line_text(file, range.side(), range.start())?;
    let last = if range.side() == range.end_side() && range.start() == range.end() {
        first
    } else {
        line_text(file, range.end_side(), range.end())?
    };
    Some([first, last])
}

fn line_text(file: &DiffFile, side: DiffSide, number: usize) -> Option<&str> {
    file.hunks
        .iter()
        .flat_map(|hunk| &hunk.lines)
        .filter_map(|line| match side {
            DiffSide::Original => line.old.as_ref(),
            DiffSide::Modified => line.new.as_ref(),
        })
        .find(|line| line.number == number)
        .map(|line| line.text.as_str())
}

fn review_prompt(comments: &[DiffComment], cx: &App) -> Option<Prompt> {
    let mut prompt = String::new();
    for comment in comments {
        let value = comment.input.read(cx).value();
        let body = value.trim();
        if body.is_empty() {
            continue;
        }
        if prompt.is_empty() {
            prompt.push_str(
                "Please address these comments on the checkout diff. \
                 Paths are relative to the Git repository root. \
                 Line numbers refer to the reviewed diff.\n",
            );
        }
        writeln!(prompt, "\n- {}", comment.location()).unwrap();
        if !comment.attached {
            let [first, last] = &comment.source;
            let excerpt = |line: &str| line.trim().chars().take(160).collect::<String>();
            writeln!(prompt, "  Reviewed start line was {:?}", excerpt(first)).unwrap();
            if first != last {
                writeln!(prompt, "  Reviewed end line was {:?}", excerpt(last)).unwrap();
            }
        }
        for line in body.lines() {
            writeln!(prompt, "  {line}").unwrap();
        }
    }
    (!prompt.is_empty()).then(|| prompt.trim_end().to_owned().into())
}

impl Workspace {
    pub(super) fn diff_review_session(&self) -> Option<(SessionLocation, &AgentView)> {
        let session = self
            .view
            .displayed_session()
            .filter(|_| self.diff.visible)?;
        let project = &self.projects[session.project_index];
        let agent = &project.agents[session.agent_index];
        (project.ssh_host.is_none() && !agent.config.terminal && !agent.config.archived)
            .then_some((session, agent))
    }

    pub(super) fn add_diff_comment(
        &mut self,
        agent_id: u64,
        range: &DiffLineRange,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.diff_review_session().map(|(_, agent)| agent.config.id) != Some(agent_id) {
            return;
        }
        let Some(source) = range_source(&self.diff.files, range) else {
            return;
        };
        let comments = self.diff.comments.entry(agent_id).or_default();
        if comments
            .iter()
            .any(|comment| comment.range == *range && comment.source == source)
        {
            return;
        }
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(2)
                .placeholder("What should the agent change?")
        });
        let subscription = cx.subscribe(&input, |_, _, event, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        });
        comments.push(DiffComment {
            range: range.clone(),
            source: source.map(str::to_owned),
            attached: true,
            input,
            _subscription: subscription,
        });
        cx.notify();
    }

    pub(super) fn remove_diff_comment(&mut self, agent_id: u64, id: u64, cx: &mut Context<Self>) {
        if let Some(comments) = self.diff.comments.get_mut(&agent_id) {
            comments.retain(|comment| comment.id() != id);
            if comments.is_empty() {
                self.diff.comments.remove(&agent_id);
            }
            cx.notify();
        }
    }

    pub(super) fn send_diff_comments(&mut self, agent_id: u64, cx: &mut Context<Self>) {
        let Some((session, agent)) = self.diff_review_session() else {
            return;
        };
        if agent.config.id != agent_id || agent.auth_required || agent.login.is_some() {
            return;
        }
        if let Some(prompt) = self
            .diff
            .comments
            .get(&agent_id)
            .and_then(|comments| review_prompt(comments, cx))
            && !self.recording_blocks_send(agent_id, cx)
            && self.submit_prompt(session, prompt, cx)
        {
            self.diff.comments.remove(&agent_id);
        }
    }
}

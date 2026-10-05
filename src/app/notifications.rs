use super::*;
use gpui_kit::SystemNotification;

#[derive(Clone, Copy)]
pub(super) struct Attention {
    finished: bool,
    requests: usize,
}

impl Workspace {
    pub(super) fn agent_location(&self, agent_id: u64) -> Option<SessionLocation> {
        self.projects
            .iter()
            .enumerate()
            .find_map(|(project_index, project)| {
                project
                    .agents
                    .iter()
                    .position(|agent| agent.config.id == agent_id)
                    .map(|agent_index| SessionLocation {
                        project_index,
                        agent_index,
                    })
            })
    }

    pub(super) fn attention(&self, agent_id: u64) -> Option<Attention> {
        self.agent_location(agent_id).map(|location| {
            let agent = &self.projects[location.project_index].agents[location.agent_index];
            Attention {
                finished: agent.status == Status::Done,
                requests: agent.elicitations.len() + agent.permissions.len(),
            }
        })
    }

    pub(super) fn notify_attention(
        &self,
        agent_id: u64,
        before: Option<Attention>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(before), Some(after)) = (before, self.attention(agent_id)) else {
            return;
        };
        let body = if after.requests > before.requests {
            "Needs your input"
        } else if after.finished && !before.finished {
            "Finished"
        } else {
            return;
        };
        let Some(location) = self.agent_location(agent_id) else {
            return;
        };
        let on_screen = window.is_window_active()
            && self.settings.is_none()
            && self.view.displayed_session() == Some(location);
        if !self.notifications || on_screen {
            return;
        }
        let project = &self.projects[location.project_index];
        let agent = &project.agents[location.agent_index];
        let project_name = project.path.file_name().map_or_else(
            || project.path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        let title = agent.config.session_title().map_or_else(
            || project_name.clone(),
            |session_title| format!("{project_name}: {session_title}"),
        );
        cx.show_system_notification(SystemNotification {
            tag: agent_id.to_string().into(),
            title: title.into(),
            body: body.into(),
            actions: Vec::new(),
        });
    }

    pub(super) fn open_notified_session(
        &mut self,
        tag: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(location) = tag
            .parse()
            .ok()
            .and_then(|agent_id| self.agent_location(agent_id))
        else {
            return;
        };
        self.commit_text_settings(cx);
        self.settings = None;
        self.set_view(WorkspaceView::Conversation(location), window, cx);
        window.activate_window();
        cx.notify();
    }

    pub(super) fn set_notifications(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if enabled == self.notifications {
            return;
        }
        self.notifications = enabled;
        self.persist();
        cx.notify();
    }
}

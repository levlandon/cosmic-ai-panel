//! Panel window lifecycle and shell surface helpers.

use super::*;

impl AppModel {
    pub(in crate::app) fn open_panel(&mut self) -> Task<cosmic::Action<Message>> {
        let id = Id::unique();
        self.panel_window = Some(id);
        self.panel_requested_open = true;
        self.messages_bottom_distance = 0.0;

        Task::batch([
            get_layer_surface::<cosmic::Action<Message>>(SctkLayerSurfaceSettings {
                id,
                layer: Layer::Top,
                keyboard_interactivity: KeyboardInteractivity::OnDemand,
                anchor: Anchor::RIGHT.union(Anchor::TOP).union(Anchor::BOTTOM),
                namespace: <Self as cosmic::Application>::APP_ID.to_string(),
                margin: panel_reserved_margin(&self.core),
                size: Some((Some(PANEL_WIDTH as u32), None)),
                exclusive_zone: -1,
                size_limits: Limits::NONE
                    .min_width(PANEL_WIDTH)
                    .min_height(PANEL_MIN_HEIGHT),
                ..Default::default()
            }),
            self.scroll_messages_to_end(true),
        ])
    }

    pub(in crate::app) fn handle_applet_pressed(&mut self) -> Task<cosmic::Action<Message>> {
        match self.panel_window {
            Some(id) => {
                self.panel_requested_open = false;
                self.panel_window = None;
                self.panel_view = PanelView::Chat;
                self.hovered_message_id = None;
                destroy_layer_surface(id)
            }
            None => self.open_panel(),
        }
    }

    pub(in crate::app) fn toggle_panel(&mut self) -> Task<cosmic::Action<Message>> {
        if let Some(id) = self.panel_window.take() {
            self.panel_requested_open = false;
            self.panel_view = PanelView::Chat;
            self.reset_inline_edit();
            self.hovered_message_id = None;
            destroy_layer_surface(id)
        } else {
            self.open_panel()
        }
    }

    pub(in crate::app) fn handle_escape_pressed(
        &mut self,
        id: Id,
    ) -> Task<cosmic::Action<Message>> {
        if self.panel_window == Some(id) {
            if self.panel_view != PanelView::Chat {
                self.panel_view = PanelView::Chat;
            } else {
                self.panel_requested_open = false;
                self.panel_window = None;
                self.reset_inline_edit();
                self.hovered_message_id = None;
                return destroy_layer_surface(id);
            }
        }

        Task::none()
    }

    pub(in crate::app) fn handle_panel_closed(
        &mut self,
        id: Id,
    ) -> Task<cosmic::Action<Message>> {
        if self.panel_window == Some(id) {
            self.panel_window = None;
            if self.panel_requested_open {
                return self.open_panel();
            }
        }

        Task::none()
    }
}

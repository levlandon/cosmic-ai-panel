// SPDX-License-Identifier: MPL-2.0
//! Shared visual styling helpers for panel widgets and message surfaces.

use super::*;

pub(in crate::app) fn send_button_class() -> cosmic::theme::Button {
    cosmic::theme::Button::Custom {
        active: Box::new(|focused, theme| send_button_style(focused, theme, 1.0)),
        disabled: Box::new(|theme| send_button_style(false, theme, 0.7)),
        hovered: Box::new(|focused, theme| send_button_style(focused, theme, 0.94)),
        pressed: Box::new(|focused, theme| send_button_style(focused, theme, 0.88)),
    }
}

pub(in crate::app) fn composer_container_class() -> cosmic::theme::Container<'static> {
    cosmic::theme::Container::Custom(Box::new(|theme| {
        let cosmic = theme.cosmic();
        cosmic::iced::widget::container::Style {
            icon_color: Some(Color::from(theme.current_container().on)),
            text_color: Some(Color::from(theme.current_container().on)),
            background: Some(Background::Color(
                theme.current_container().component.base.into(),
            )),
            border: cosmic::iced_core::Border {
                radius: cosmic.corner_radii.radius_l.into(),
                width: 1.0,
                color: theme.current_container().component.divider.into(),
            },
            shadow: Default::default(),
            snap: true,
        }
    }))
}

pub(in crate::app) fn composer_editor_class() -> cosmic::theme::iced::TextEditor<'static> {
    cosmic::theme::iced::TextEditor::Custom(Box::new(|theme, _status| {
        let cosmic = theme.cosmic();

        cosmic::iced::widget::text_editor::Style {
            background: Background::Color(Color::TRANSPARENT),
            border: cosmic::iced_core::Border {
                radius: cosmic.corner_radii.radius_0.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
            placeholder: cosmic.palette.neutral_9.with_alpha(0.7).into(),
            value: theme.current_container().on.into(),
            selection: cosmic.accent.base.with_alpha(0.3).into(),
        }
    }))
}

pub(in crate::app) fn message_viewer_class(
    value_color: Color,
) -> cosmic::theme::iced::TextEditor<'static> {
    cosmic::theme::iced::TextEditor::Custom(Box::new(move |theme, _status| {
        let cosmic = theme.cosmic();

        cosmic::iced::widget::text_editor::Style {
            background: Background::Color(Color::TRANSPARENT),
            border: cosmic::iced_core::Border {
                radius: cosmic.corner_radii.radius_0.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
            placeholder: Color::TRANSPARENT,
            value: value_color,
            selection: cosmic.accent.base.with_alpha(0.32).into(),
        }
    }))
}

pub(in crate::app) fn message_action_button_class(visible: bool) -> cosmic::theme::Button {
    if visible {
        cosmic::theme::Button::Icon
    } else {
        cosmic::theme::Button::Custom {
            active: Box::new(|_, theme| hidden_message_action_button_style(theme)),
            disabled: Box::new(hidden_message_action_button_style),
            hovered: Box::new(|_, theme| hidden_message_action_button_style(theme)),
            pressed: Box::new(|_, theme| hidden_message_action_button_style(theme)),
        }
    }
}

fn hidden_message_action_button_style(theme: &cosmic::Theme) -> cosmic::widget::button::Style {
    let cosmic = theme.cosmic();

    cosmic::widget::button::Style {
        shadow_offset: Vector::default(),
        background: None,
        overlay: None,
        border_radius: cosmic.corner_radii.radius_m.into(),
        border_width: 0.0,
        border_color: Color::TRANSPARENT,
        outline_width: 0.0,
        outline_color: Color::TRANSPARENT,
        icon_color: Some(Color::TRANSPARENT),
        text_color: Some(Color::TRANSPARENT),
    }
}

pub(in crate::app) fn chat_list_card_class() -> cosmic::theme::Container<'static> {
    cosmic::theme::Container::Custom(Box::new(|theme| {
        let cosmic = theme.cosmic();
        let background = theme.current_container().component.base;
        let border = theme.current_container().component.divider;

        cosmic::iced::widget::container::Style {
            icon_color: Some(Color::from(theme.current_container().on)),
            text_color: Some(Color::from(theme.current_container().on)),
            background: Some(Background::Color(background.into())),
            border: cosmic::iced_core::Border {
                radius: cosmic.corner_radii.radius_l.into(),
                width: 1.0,
                color: border.into(),
            },
            shadow: Default::default(),
            snap: true,
        }
    }))
}

pub(in crate::app) fn settings_modal_backdrop_class() -> cosmic::theme::Container<'static> {
    cosmic::theme::Container::Custom(Box::new(|_theme| cosmic::iced::widget::container::Style {
        icon_color: None,
        text_color: None,
        background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.42))),
        border: cosmic::iced_core::Border {
            radius: 0.0.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        shadow: Default::default(),
        snap: true,
    }))
}

pub(in crate::app) fn loading_dot_class() -> cosmic::theme::Container<'static> {
    cosmic::theme::Container::Custom(Box::new(|_theme| cosmic::iced::widget::container::Style {
        icon_color: Some(Color::WHITE),
        text_color: Some(Color::WHITE),
        background: Some(Background::Color(Color::WHITE)),
        border: cosmic::iced_core::Border {
            radius: 999.0.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        shadow: Default::default(),
        snap: true,
    }))
}

pub(in crate::app) fn error_notice_class() -> cosmic::theme::Container<'static> {
    cosmic::theme::Container::Custom(Box::new(|theme| {
        let cosmic = theme.cosmic();
        let background = Color::from_rgba(0.36, 0.08, 0.08, 0.32);
        let border = Color::from_rgba(1.0, 0.42, 0.42, 0.28);

        cosmic::iced::widget::container::Style {
            icon_color: Some(Color::from_rgb(1.0, 0.42, 0.42)),
            text_color: Some(Color::from(theme.current_container().on)),
            background: Some(Background::Color(background)),
            border: cosmic::iced_core::Border {
                radius: cosmic.corner_radii.radius_xl.into(),
                width: 1.0,
                color: border,
            },
            shadow: Default::default(),
            snap: true,
        }
    }))
}

pub(in crate::app) fn transient_chat_notice_class() -> cosmic::theme::Container<'static> {
    cosmic::theme::Container::Custom(Box::new(|theme| {
        let cosmic = theme.cosmic();

        cosmic::iced::widget::container::Style {
            icon_color: Some(Color::from_rgba(1.0, 1.0, 1.0, 0.82)),
            text_color: Some(Color::from_rgba(1.0, 1.0, 1.0, 0.82)),
            background: Some(Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.08))),
            border: cosmic::iced_core::Border {
                radius: cosmic.corner_radii.radius_xl.into(),
                width: 1.0,
                color: Color::from_rgba(1.0, 1.0, 1.0, 0.12),
            },
            shadow: Default::default(),
            snap: true,
        }
    }))
}

pub(in crate::app) fn composer_input_class() -> cosmic::theme::TextInput {
    cosmic::theme::TextInput::Custom {
        active: Box::new(|theme: &cosmic::Theme| composer_input_appearance(theme)),
        error: Box::new(|theme: &cosmic::Theme| composer_input_appearance(theme)),
        hovered: Box::new(|theme: &cosmic::Theme| composer_input_appearance(theme)),
        focused: Box::new(|theme: &cosmic::Theme| composer_input_appearance(theme)),
        disabled: Box::new(|theme: &cosmic::Theme| composer_input_appearance(theme)),
    }
}

fn composer_input_appearance(theme: &cosmic::Theme) -> cosmic::widget::text_input::Appearance {
    let cosmic = theme.cosmic();

    cosmic::widget::text_input::Appearance {
        background: Color::TRANSPARENT.into(),
        border_radius: cosmic.corner_radii.radius_0.into(),
        border_width: 0.0,
        border_offset: None,
        border_color: Color::TRANSPARENT,
        icon_color: None,
        text_color: Some(theme.current_container().on.into()),
        placeholder_color: cosmic.palette.neutral_9.with_alpha(0.7).into(),
        selected_text_color: cosmic.on_accent_color().into(),
        selected_fill: cosmic.accent_color().into(),
        label_color: cosmic.palette.neutral_9.into(),
    }
}

pub(in crate::app) fn chat_bubble_class(_user: bool) -> cosmic::theme::Container<'static> {
    cosmic::theme::Container::Custom(Box::new(move |theme| {
        let cosmic = theme.cosmic();
        let background: Color = theme.current_container().component.base.into();
        let text_color: Color = theme.current_container().on.into();
        let border_color: Color = theme.current_container().component.divider.into();

        cosmic::iced::widget::container::Style {
            icon_color: Some(text_color),
            text_color: Some(text_color),
            background: Some(Background::Color(background)),
            border: cosmic::iced_core::Border {
                radius: cosmic.corner_radii.radius_l.into(),
                width: 1.0,
                color: border_color,
            },
            shadow: Default::default(),
            snap: true,
        }
    }))
}

pub(in crate::app) fn chat_row_button_class(selected: bool) -> cosmic::theme::Button {
    cosmic::theme::Button::Custom {
        active: Box::new(move |focused, theme| chat_row_button_style(focused, theme, selected, 0)),
        disabled: Box::new(|theme| {
            let base = theme.active(false, false, &cosmic::theme::Button::Text);
            cosmic::widget::button::Style {
                shadow_offset: Vector::default(),
                background: None,
                overlay: None,
                border_radius: theme.cosmic().corner_radii.radius_l.into(),
                border_width: 0.0,
                border_color: Color::TRANSPARENT,
                outline_width: 0.0,
                outline_color: Color::TRANSPARENT,
                icon_color: base.icon_color,
                text_color: base.text_color,
            }
        }),
        hovered: Box::new(move |focused, theme| chat_row_button_style(focused, theme, selected, 1)),
        pressed: Box::new(move |focused, theme| chat_row_button_style(focused, theme, selected, 2)),
    }
}

fn chat_row_button_style(
    focused: bool,
    theme: &cosmic::Theme,
    selected: bool,
    state: u8,
) -> cosmic::widget::button::Style {
    let cosmic = theme.cosmic();
    let base = theme.active(focused, selected, &cosmic::theme::Button::Text);
    let hover_bg = theme.current_container().component.hover;
    let pressed_bg = theme.current_container().component.pressed;
    let selected_bg = cosmic.accent.base.with_alpha(0.18);
    let background = if selected {
        selected_bg.into()
    } else {
        match state {
            1 => hover_bg.into(),
            2 => pressed_bg.into(),
            _ => Color::TRANSPARENT,
        }
    };
    let (outline_width, outline_color) = if focused {
        (1.0, cosmic.accent_color().into())
    } else {
        (0.0, Color::TRANSPARENT)
    };

    cosmic::widget::button::Style {
        shadow_offset: Vector::default(),
        background: Some(Background::Color(background)),
        overlay: None,
        border_radius: cosmic.corner_radii.radius_l.into(),
        border_width: 0.0,
        border_color: Color::TRANSPARENT,
        outline_width,
        outline_color,
        icon_color: base.icon_color,
        text_color: if selected {
            Some(cosmic.accent_text_color().into())
        } else {
            base.text_color
        },
    }
}

fn send_button_style(
    focused: bool,
    theme: &cosmic::Theme,
    shade: f32,
) -> cosmic::widget::button::Style {
    let cosmic = theme.cosmic();
    let (outline_width, outline_color) = if focused {
        (1.0, cosmic.accent_color().into())
    } else {
        (0.0, Color::TRANSPARENT)
    };

    cosmic::widget::button::Style {
        shadow_offset: Vector::default(),
        background: Some(Background::Color(Color::from_rgb(shade, shade, shade))),
        overlay: None,
        border_radius: cosmic.corner_radii.radius_xl.into(),
        border_width: 0.0,
        border_color: Color::TRANSPARENT,
        outline_width,
        outline_color,
        icon_color: Some(Color::BLACK),
        text_color: Some(Color::BLACK),
    }
}

pub(in crate::app) fn thin_vertical_scrollbar() -> cosmic::iced::widget::scrollable::Direction {
    cosmic::iced::widget::scrollable::Direction::Vertical(
        cosmic::iced::widget::scrollable::Scrollbar::new()
            .width(THIN_SCROLLBAR_WIDTH)
            .scroller_width(THIN_SCROLLER_WIDTH)
            .spacing(2),
    )
}

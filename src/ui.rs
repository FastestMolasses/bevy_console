use crate::{
    ConsoleCommandEntered, ConsoleConfiguration, ConsoleOpen, ConsoleState, ToggleConsoleKey,
};
use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use bevy_egui::egui::{self, Align, Frame, ScrollArea, TextEdit};
use bevy_egui::egui::{text::LayoutJob, text_edit::CCursorRange};
use bevy_egui::egui::{Context, Id};
use bevy_egui::{
    egui::{epaint::text::cursor::CCursor, Color32, FontId, TextFormat},
    EguiContexts,
};
use clap::builder::StyledStr;
use shlex::Shlex;

pub(crate) fn console_ui(
    mut egui_context: EguiContexts,
    config: Res<ConsoleConfiguration>,
    mut keyboard_input_events: EventReader<KeyboardInput>,
    keys: Res<Input<KeyCode>>,
    mut state: ResMut<ConsoleState>,
    mut command_entered: EventWriter<ConsoleCommandEntered>,
    mut console_open: ResMut<ConsoleOpen>,
) {
    let keyboard_input_events = keyboard_input_events.iter().collect::<Vec<_>>();
    let ctx = egui_context.ctx_mut();

    let pressed = keyboard_input_events
        .iter()
        .any(|code| console_key_pressed(code, &config.keys));

    // Always close if console open
    // Avoid opening console if typing in another text input
    if pressed && (console_open.open || !ctx.wants_keyboard_input()) {
        console_open.open = !console_open.open;
    }

    if console_open.open {
        egui::Window::new("console")
            .collapsible(false)
            .fixed_pos([config.left_pos, config.top_pos])
            .default_size([config.width, config.height])
            .resizable(false)
            .title_bar(false)
            .frame(egui::Frame::none().fill(egui::Color32::from_black_alpha(60)))
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    let scroll_height = ui.available_height() - 30.0;

                    // Scroll area
                    ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .stick_to_bottom(true)
                        .max_height(scroll_height)
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                for line in &state.scrollback {
                                    let mut text = LayoutJob::default();

                                    text.append(
                                        &line.to_string(), //TOOD: once clap supports custom styling use it here
                                        0f32,
                                        TextFormat::simple(FontId::monospace(14f32), Color32::GRAY),
                                    );

                                    ui.label(text);
                                }
                            });

                            // Scroll to bottom if console just opened
                            if console_open.is_changed() {
                                ui.scroll_to_cursor(Some(Align::BOTTOM));
                            }
                        });

                    // Separator
                    ui.separator();

                    // Input
                    let text_edit = TextEdit::singleline(&mut state.buf)
                        .desired_width(f32::INFINITY)
                        .lock_focus(true)
                        .frame(false)
                        .font(egui::TextStyle::Monospace);

                    // Handle enter
                    let text_edit_response = ui.add(text_edit);
                    if text_edit_response.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    {
                        if state.buf.trim().is_empty() {
                            state.scrollback.push(StyledStr::new());
                        } else {
                            let msg = format!("{}{}", config.symbol, state.buf);
                            state.scrollback.push(msg.into());
                            let cmd_string = state.buf.clone();
                            state.history.insert(1, cmd_string.into());
                            if state.history.len() > config.history_size + 1 {
                                state.history.pop_back();
                            }

                            let mut args = Shlex::new(&state.buf).collect::<Vec<_>>();

                            if !args.is_empty() {
                                let command_name = args.remove(0);
                                debug!("Command entered: `{command_name}`, with args: `{args:?}`");

                                let command = config.commands.get(command_name.as_str());

                                if command.is_some() {
                                    command_entered
                                        .send(ConsoleCommandEntered { command_name, args });
                                } else {
                                    // TODO: IF COMMAND IS NOT RECOGNIZED, CHECK IF IT'S SETTING A VARIABLE
                                    debug!(
                                        "Command not recognized, recognized commands: `{:?}`",
                                        config.commands.keys().collect::<Vec<_>>()
                                    );

                                    state.scrollback.push("error: Invalid command".into());
                                }
                            }

                            state.buf.clear();
                        }
                    }

                    // Clear on ctrl+l
                    if keyboard_input_events
                        .iter()
                        .any(|&k| k.state.is_pressed() && k.key_code == Some(KeyCode::L))
                        && (keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]))
                    {
                        state.scrollback.clear();
                    }

                    // Handle up and down through history
                    if text_edit_response.has_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::ArrowUp))
                        && state.history.len() > 1
                        && state.history_index < state.history.len() - 1
                    {
                        if state.history_index == 0 && !state.buf.trim().is_empty() {
                            *state.history.get_mut(0).unwrap() = state.buf.clone().into();
                        }

                        state.history_index += 1;
                        let previous_item = state.history.get(state.history_index).unwrap().clone();
                        state.buf = previous_item.to_string();

                        set_cursor_pos(ui.ctx(), text_edit_response.id, state.buf.len());
                    } else if text_edit_response.has_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::ArrowDown))
                        && state.history_index > 0
                    {
                        state.history_index -= 1;
                        let next_item = state.history.get(state.history_index).unwrap().clone();
                        state.buf = next_item.to_string();

                        set_cursor_pos(ui.ctx(), text_edit_response.id, state.buf.len());
                    }

                    // Focus on input
                    ui.memory_mut(|m| m.request_focus(text_edit_response.id));
                });
            });
    }
}

fn console_key_pressed(
    keyboard_input: &KeyboardInput,
    configured_keys: &[ToggleConsoleKey],
) -> bool {
    if !keyboard_input.state.is_pressed() {
        return false;
    }

    for configured_key in configured_keys {
        match configured_key {
            ToggleConsoleKey::KeyCode(configured_key_code) => match keyboard_input.key_code {
                None => continue,
                Some(pressed_key) => {
                    if configured_key_code == &pressed_key {
                        return true;
                    }
                }
            },
            ToggleConsoleKey::ScanCode(configured_scan_code) => {
                if &keyboard_input.scan_code == configured_scan_code {
                    return true;
                }
            }
        }
    }

    false
}

fn set_cursor_pos(ctx: &Context, id: Id, pos: usize) {
    if let Some(mut state) = TextEdit::load_state(ctx, id) {
        state.set_ccursor_range(Some(CCursorRange::one(CCursor::new(pos))));
        state.store(ctx, id);
    }
}

#[cfg(test)]
mod tests {
    use bevy::input::ButtonState;

    use super::*;

    #[test]
    fn test_console_key_pressed_scan_code() {
        let input = KeyboardInput {
            scan_code: 41,
            key_code: None,
            state: ButtonState::Pressed,
            window: Entity::PLACEHOLDER,
        };

        let config = vec![ToggleConsoleKey::ScanCode(41)];

        let result = console_key_pressed(&input, &config);
        assert!(result);
    }

    #[test]
    fn test_console_wrong_key_pressed_scan_code() {
        let input = KeyboardInput {
            scan_code: 42,
            key_code: None,
            state: ButtonState::Pressed,
            window: Entity::PLACEHOLDER,
        };

        let config = vec![ToggleConsoleKey::ScanCode(41)];

        let result = console_key_pressed(&input, &config);
        assert!(!result);
    }

    #[test]
    fn test_console_key_pressed_key_code() {
        let input = KeyboardInput {
            scan_code: 0,
            key_code: Some(KeyCode::Grave),
            state: ButtonState::Pressed,
            window: Entity::PLACEHOLDER,
        };

        let config = vec![ToggleConsoleKey::KeyCode(KeyCode::Grave)];

        let result = console_key_pressed(&input, &config);
        assert!(result);
    }

    #[test]
    fn test_console_wrong_key_pressed_key_code() {
        let input = KeyboardInput {
            scan_code: 0,
            key_code: Some(KeyCode::A),
            state: ButtonState::Pressed,
            window: Entity::PLACEHOLDER,
        };

        let config = vec![ToggleConsoleKey::KeyCode(KeyCode::Grave)];

        let result = console_key_pressed(&input, &config);
        assert!(!result);
    }

    #[test]
    fn test_console_key_right_key_but_not_pressed() {
        let input = KeyboardInput {
            scan_code: 0,
            key_code: Some(KeyCode::Grave),
            state: ButtonState::Released,
            window: Entity::PLACEHOLDER,
        };

        let config = vec![ToggleConsoleKey::KeyCode(KeyCode::Grave)];

        let result = console_key_pressed(&input, &config);
        assert!(!result);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CCChanTrayAction {
    Show,
    Hide,
}

pub fn next_ccchan_tray_action(window_visible: bool) -> CCChanTrayAction {
    if window_visible {
        CCChanTrayAction::Hide
    } else {
        CCChanTrayAction::Show
    }
}

pub fn run_ccchan_tray_toggle<E>(
    window_visible: bool,
    mut show_window: impl FnMut() -> Result<(), E>,
    mut hide_window: impl FnMut() -> Result<(), E>,
) -> Result<CCChanTrayAction, E> {
    let action = next_ccchan_tray_action(window_visible);
    match action {
        CCChanTrayAction::Show => show_window()?,
        CCChanTrayAction::Hide => hide_window()?,
    }
    Ok(action)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ccchan_tray_action_toggles_current_visibility() {
        assert_eq!(next_ccchan_tray_action(true), CCChanTrayAction::Hide);
        assert_eq!(next_ccchan_tray_action(false), CCChanTrayAction::Show);
    }

    #[test]
    fn ccchan_tray_toggle_shows_when_saved_hidden() {
        let mut show_calls = 0;
        let mut hide_calls = 0;

        let action = run_ccchan_tray_toggle(
            false,
            || {
                show_calls += 1;
                Ok::<(), ()>(())
            },
            || {
                hide_calls += 1;
                Ok::<(), ()>(())
            },
        )
        .expect("tray toggle should succeed");

        assert_eq!(action, CCChanTrayAction::Show);
        assert_eq!(show_calls, 1);
        assert_eq!(hide_calls, 0);
    }

    #[test]
    fn ccchan_tray_toggle_hides_when_saved_visible() {
        let mut show_calls = 0;
        let mut hide_calls = 0;

        let action = run_ccchan_tray_toggle(
            true,
            || {
                show_calls += 1;
                Ok::<(), ()>(())
            },
            || {
                hide_calls += 1;
                Ok::<(), ()>(())
            },
        )
        .expect("tray toggle should succeed");

        assert_eq!(action, CCChanTrayAction::Hide);
        assert_eq!(show_calls, 0);
        assert_eq!(hide_calls, 1);
    }
}

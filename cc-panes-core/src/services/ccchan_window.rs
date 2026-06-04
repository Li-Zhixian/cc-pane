const DEFAULT_POSITION: (f64, f64) = (80.0, 80.0);
const PET_SIZE: f64 = 120.0;
const SAFE_MARGIN: f64 = 8.0;
const HALF_OFF_TOLERANCE: f64 = 40.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogicalMonitorRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl LogicalMonitorRect {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

pub fn clamp_ccchan_position_to_visible(
    monitors: &[LogicalMonitorRect],
    x: f64,
    y: f64,
) -> (f64, f64) {
    if monitors.is_empty() {
        return DEFAULT_POSITION;
    }

    let already_visible = monitors.iter().any(|monitor| {
        x + HALF_OFF_TOLERANCE > monitor.x
            && x < monitor.x + monitor.width - HALF_OFF_TOLERANCE
            && y + HALF_OFF_TOLERANCE > monitor.y
            && y < monitor.y + monitor.height - HALF_OFF_TOLERANCE
    });
    if already_visible {
        return (x, y);
    }

    let mut best: Option<(f64, f64, f64)> = None;
    for monitor in monitors {
        let cx = x.clamp(
            monitor.x + SAFE_MARGIN,
            (monitor.x + monitor.width - PET_SIZE - SAFE_MARGIN).max(monitor.x + SAFE_MARGIN),
        );
        let cy = y.clamp(
            monitor.y + SAFE_MARGIN,
            (monitor.y + monitor.height - PET_SIZE - SAFE_MARGIN).max(monitor.y + SAFE_MARGIN),
        );
        let dist = (cx - x).powi(2) + (cy - y).powi(2);
        if best.is_none_or(|current| dist < current.0) {
            best = Some((dist, cx, cy));
        }
    }
    best.map(|(_, cx, cy)| (cx, cy)).unwrap_or(DEFAULT_POSITION)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_default_position_when_no_monitors_exist() {
        assert_eq!(
            clamp_ccchan_position_to_visible(&[], 500.0, 500.0),
            (80.0, 80.0)
        );
    }

    #[test]
    fn keeps_position_when_mascot_is_visible_on_primary_monitor() {
        let monitors = [LogicalMonitorRect::new(0.0, 0.0, 1920.0, 1080.0)];

        assert_eq!(
            clamp_ccchan_position_to_visible(&monitors, 1200.0, 400.0),
            (1200.0, 400.0)
        );
    }

    #[test]
    fn keeps_position_when_mascot_is_visible_on_negative_coordinate_monitor() {
        let monitors = [
            LogicalMonitorRect::new(0.0, 0.0, 1920.0, 1080.0),
            LogicalMonitorRect::new(-2560.0, -183.0, 1707.0, 1067.0),
        ];

        assert_eq!(
            clamp_ccchan_position_to_visible(&monitors, -1800.0, 120.0),
            (-1800.0, 120.0)
        );
    }

    #[test]
    fn clamps_offscreen_position_to_nearest_negative_coordinate_monitor() {
        let monitors = [
            LogicalMonitorRect::new(0.0, 0.0, 1920.0, 1080.0),
            LogicalMonitorRect::new(-2560.0, -183.0, 1707.0, 1067.0),
        ];

        assert_eq!(
            clamp_ccchan_position_to_visible(&monitors, -4000.0, -500.0),
            (-2552.0, -175.0)
        );
    }

    #[test]
    fn clamps_offscreen_position_to_nearest_primary_monitor() {
        let monitors = [
            LogicalMonitorRect::new(0.0, 0.0, 1920.0, 1080.0),
            LogicalMonitorRect::new(-2560.0, -183.0, 1707.0, 1067.0),
        ];

        assert_eq!(
            clamp_ccchan_position_to_visible(&monitors, 2200.0, 1200.0),
            (1792.0, 952.0)
        );
    }

    #[test]
    fn uses_safe_margin_when_monitor_is_smaller_than_the_pet() {
        let monitors = [LogicalMonitorRect::new(40.0, 50.0, 80.0, 90.0)];

        assert_eq!(
            clamp_ccchan_position_to_visible(&monitors, 400.0, 500.0),
            (48.0, 58.0)
        );
    }
}

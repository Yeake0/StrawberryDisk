//! Reserve one edge inside the task-button container, leaving Explorer to lay
//! out (and truncate/overflow) application buttons in the remaining rectangle.
use super::{layout::Bounds, position::Edge};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    pub buttons: Bounds,
    pub monitor: Bounds,
}

/// A DPI/taskbar-height change may update only the cross axis before the next
/// request. Recover our reserved axis without compounding the reservation; a
/// different axis layout belongs to Explorer and must be treated as fresh.
pub fn original(current: Bounds, before: Bounds, applied: Bounds) -> Bounds {
    let vertical = before.width() < before.height();
    if vertical != (current.width() < current.height()) {
        return current;
    }
    if vertical && current.top == applied.top && current.bottom == applied.bottom {
        Bounds {
            top: before.top,
            bottom: before.bottom,
            ..current
        }
    } else if !vertical && current.left == applied.left && current.right == applied.right {
        Bounds {
            left: before.left,
            right: before.right,
            ..current
        }
    } else {
        current
    }
}

pub fn arrange(base: Bounds, width: i32, height: i32, gap: i32, edge: Edge) -> Option<Plan> {
    if base.width() < base.height() {
        return arrange(base.transpose(), height, width, gap, edge).map(|p| Plan {
            buttons: p.buttons.transpose(),
            monitor: p.monitor.transpose(),
        });
    }
    if width <= 0 || height <= 0 || gap <= 0 || height > base.height() {
        return None;
    }
    let used = width.checked_add(gap.checked_mul(2)?)?;
    // Keep at least one ordinary task-button width for Explorer's overflow UI.
    if base.width().checked_sub(used)? < gap.checked_mul(12)?.max(base.height().checked_add(1)?) {
        return None;
    }
    let mut buttons = base;
    let left = match edge {
        Edge::Left => {
            buttons.left = base.left.checked_add(used)?;
            base.left.checked_add(gap)?
        }
        Edge::Right => {
            buttons.right = base.right.checked_sub(used)?;
            buttons.right.checked_add(gap)?
        }
    };
    let top = base.top + (base.height() - height) / 2;
    Some(Plan {
        buttons,
        monitor: Bounds {
            left,
            top,
            right: left + width,
            bottom: top + height,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn base() -> Bounds {
        Bounds {
            left: 2,
            top: 0,
            right: 1602,
            bottom: 80,
        }
    }
    #[test]
    fn full_task_buttons_get_smaller_without_covering_the_monitor() {
        for edge in [Edge::Left, Edge::Right] {
            let p = arrange(base(), 200, 72, 8, edge).unwrap();
            assert_eq!(p.buttons.width(), 1384);
            assert!(p.buttons.fits_in(base()) && p.monitor.fits_in(base()));
            assert!(p.monitor.right < p.buttons.left || p.buttons.right < p.monitor.left);
        }
    }

    #[test]
    fn cross_axis_dpi_changes_do_not_compound_or_lose_the_reservation() {
        let p = arrange(base(), 200, 72, 8, Edge::Right).unwrap();
        let resized = Bounds {
            bottom: 60,
            ..p.buttons
        };
        let recovered = original(resized, base(), p.buttons);
        assert_eq!(
            recovered,
            Bounds {
                bottom: 60,
                ..base()
            }
        );
        assert_eq!(original(base(), base(), p.buttons), base());
        assert_eq!(
            original(base().transpose(), base(), p.buttons),
            base().transpose()
        );
    }
    #[test]
    fn dpi_scaling_and_vertical_edges_preserve_the_same_reservation() {
        let a = arrange(base(), 200, 72, 8, Edge::Left).unwrap();
        let vertical = arrange(base().transpose(), 72, 200, 8, Edge::Left).unwrap();
        assert_eq!(vertical.buttons, a.buttons.transpose());
        assert_eq!(vertical.monitor, a.monitor.transpose());
        let scaled = Bounds {
            left: 4,
            top: 0,
            right: 3204,
            bottom: 160,
        };
        let b = arrange(scaled, 400, 144, 16, Edge::Left).unwrap();
        assert_eq!(b.buttons.width(), 2 * a.buttons.width());
        assert_eq!(b.monitor.left, 2 * a.monitor.left);
    }
    #[test]
    fn invalid_sizes_do_not_consume_all_of_explorer_or_overflow() {
        for (w, h, gap) in [
            (0, 72, 8),
            (1600, 72, 8),
            (200, 81, 8),
            (200, 72, 0),
            (i32::MAX, 72, 8),
        ] {
            assert!(arrange(base(), w, h, gap, Edge::Right).is_none());
        }
    }
}

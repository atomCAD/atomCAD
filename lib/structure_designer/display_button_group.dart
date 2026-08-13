import 'package:flutter/material.dart';
import '../common/ui_common.dart';

/// Shared building blocks for the DISPLAY panel's bar of icon buttons.
///
/// The panel's buttons come in two kinds: *radio groups*, where exactly one
/// member is active (the three geometry rendering methods), and *toggles*,
/// which are independent on/off flags (show axes). Both kinds paint a selected
/// button the same way — filled with the accent color — so a group that mixed
/// them would be ambiguous: you could not tell "Solid mode is selected" from
/// "the geometry shell is on".
///
/// Hence the rule this file exists to enforce:
///
/// 1. A [DisplayButtonGroup] is **never mixed** — it is either one radio group
///    or a run of toggles.
/// 2. Adjacent groups are **always** separated by a vertical rule. Callers do
///    not draw separators; [DisplayGroupBar] inserts them, which is why the
///    panel can no longer grow a missing one.
/// 3. Groups that concern the same subject (the geometry rendering methods and
///    the geometry shell toggle) are bundled into a [DisplayGroupCluster], and
///    [DisplayGroupBar] keeps a cluster's groups on one line when it wraps.
///
/// Grouping is by *subject*, not by control kind: a toggle sits next to the
/// radio group it qualifies rather than in a bucket of unrelated booleans.

/// Icon size inside a display button.
const double _BUTTON_ICON_SIZE = 20.0;

/// Vertical padding around the icon — also the height of its selected fill
/// beyond the icon.
const double _BUTTON_PADDING_V = 2.0;

/// Horizontal padding around the icon. Narrower than the vertical padding
/// because the horizontal budget is spent on [_BUTTON_GAP] instead; the icon
/// glyphs carry their own side bearing, so the fill does not look tight.
const double _BUTTON_PADDING_H = 1.0;

/// Gap between two buttons *within* a group. Without it the fills of two
/// selected neighbours (axes + grid) touch and read as one wide button.
///
/// It is deliberately much smaller than the gap a separator makes (2px against
/// 7px), so adding air between buttons does not dilute the grouping the
/// separators establish. Taken out of the horizontal padding rather than added
/// on top, so [_BUTTON_EXTENT] — and therefore where the bar breaks lines —
/// is unchanged.
const double _BUTTON_GAP = 2.0;

/// Total horizontal extent of one display button. Used to lay the bar out
/// without measuring, so it must stay in sync with the three constants above.
const double _BUTTON_EXTENT =
    _BUTTON_ICON_SIZE + 2 * _BUTTON_PADDING_H + _BUTTON_GAP;

/// Horizontal margin on each side of a group separator. The gap it makes is
/// this plus the neighbouring buttons' half-gaps — 11px against the 2px
/// [_BUTTON_GAP] inside a group. That ratio is the whole point: it is what
/// makes a group read as one thing, so widen [_BUTTON_GAP] and this has to
/// grow with it. Unlike [_BUTTON_GAP] it cannot be paid for out of the button
/// padding, so raising it costs real width and can push the bar onto an extra
/// line at a narrow sidebar.
const double _SEPARATOR_MARGIN = 5.0;

/// Total horizontal extent of a group separator (1px rule plus its margins).
const double _SEPARATOR_EXTENT = 1.0 + 2 * _SEPARATOR_MARGIN;

/// Vertical gap between wrapped lines of the bar.
const double _LINE_SPACING = 8.0;

/// A small icon button in the DISPLAY panel.
///
/// Used for both radio-group members and toggles; [isSelected] means "this
/// method is chosen" for the former and "this flag is on" for the latter.
/// Pass a `key` to make the button findable in integration tests.
class DisplayIconButton extends StatelessWidget {
  final IconData icon;
  final String tooltip;
  final bool isSelected;

  /// A disabled button still renders (dimmed) and keeps its tooltip, which is
  /// where the reason for being disabled belongs.
  final bool enabled;
  final VoidCallback? onPressed;

  const DisplayIconButton({
    super.key,
    required this.icon,
    required this.tooltip,
    required this.isSelected,
    this.enabled = true,
    required this.onPressed,
  });

  @override
  Widget build(BuildContext context) {
    // The gap is outside the Material, so it separates the fills without
    // shrinking the tap target's visible bounds.
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: _BUTTON_GAP / 2),
      child: Tooltip(
        message: tooltip,
        child: Material(
          color: isSelected ? AppColors.primaryAccent : Colors.transparent,
          shape:
              RoundedRectangleBorder(borderRadius: BorderRadius.circular(4.0)),
          child: InkWell(
            borderRadius: BorderRadius.circular(4.0),
            onTap: enabled ? onPressed : null,
            child: Padding(
              padding: const EdgeInsets.symmetric(
                horizontal: _BUTTON_PADDING_H,
                vertical: _BUTTON_PADDING_V,
              ),
              child: Icon(
                icon,
                size: _BUTTON_ICON_SIZE,
                color: isSelected
                    ? Colors.white
                    : (enabled ? Colors.grey[700] : Colors.grey[400]),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// One run of buttons rendered without internal separators: either a radio
/// group or a run of toggles, never a mix of the two.
class DisplayButtonGroup {
  final List<DisplayIconButton> buttons;

  const DisplayButtonGroup(this.buttons);

  double get width => buttons.length * _BUTTON_EXTENT;
}

/// Groups belonging to one subject, which [DisplayGroupBar] tries to keep on a
/// single line.
class DisplayGroupCluster {
  final List<DisplayButtonGroup> groups;

  const DisplayGroupCluster(this.groups);

  double get width =>
      groups.fold(0.0, (total, group) => total + group.width) +
      (groups.length - 1) * _SEPARATOR_EXTENT;
}

/// Lays [clusters] out as one or more lines of groups, separated by vertical
/// rules, wrapping to fit the available width.
///
/// Line breaking is computed from the fixed button extent rather than measured,
/// so the bar reflows as the sidebar is resized instead of overflowing (the
/// hardcoded `Row`s this replaced could not narrow past their content). A
/// cluster moves to the next line whole; a cluster too wide for a line of its
/// own falls back to breaking between its groups.
class DisplayGroupBar extends StatelessWidget {
  final List<DisplayGroupCluster> clusters;

  const DisplayGroupBar({super.key, required this.clusters});

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final lines = _packIntoLines(constraints.maxWidth);
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            for (var i = 0; i < lines.length; i++) ...[
              if (i > 0) const SizedBox(height: _LINE_SPACING),
              Row(
                  mainAxisSize: MainAxisSize.min,
                  children: _lineChildren(lines[i])),
            ],
          ],
        );
      },
    );
  }

  List<List<DisplayButtonGroup>> _packIntoLines(double maxWidth) {
    final lines = <List<DisplayButtonGroup>>[];
    var line = <DisplayButtonGroup>[];
    var lineWidth = 0.0;

    void place(DisplayButtonGroup group) {
      final needed = group.width + (line.isEmpty ? 0.0 : _SEPARATOR_EXTENT);
      if (line.isNotEmpty && lineWidth + needed > maxWidth) {
        lines.add(line);
        line = <DisplayButtonGroup>[];
        lineWidth = 0.0;
      }
      lineWidth += line.isEmpty ? group.width : needed;
      line.add(group);
    }

    for (final cluster in clusters) {
      final needed = cluster.width + (line.isEmpty ? 0.0 : _SEPARATOR_EXTENT);
      if (line.isNotEmpty && lineWidth + needed > maxWidth) {
        lines.add(line);
        line = <DisplayButtonGroup>[];
        lineWidth = 0.0;
      }
      // Placing group by group keeps a cluster wider than the whole bar from
      // overflowing; when the cluster fits, no break happens here.
      for (final group in cluster.groups) {
        place(group);
      }
    }
    if (line.isNotEmpty) {
      lines.add(line);
    }
    return lines;
  }

  List<Widget> _lineChildren(List<DisplayButtonGroup> line) {
    final children = <Widget>[];
    for (final group in line) {
      if (children.isNotEmpty) {
        children.add(const _GroupSeparator());
      }
      children.addAll(group.buttons);
    }
    return children;
  }
}

class _GroupSeparator extends StatelessWidget {
  const _GroupSeparator();

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 1,
      height: 20,
      margin: const EdgeInsets.symmetric(horizontal: _SEPARATOR_MARGIN),
      color: Colors.grey[400],
    );
  }
}

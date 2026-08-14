import 'package:flutter/material.dart';
import 'package:flutter_cad/common/error_display.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/structure_designer/namespace_utils.dart';

/// A header strip naming a user type by its **qualified** name, greying the
/// namespace and emphasising the simple name (issue #207).
///
/// **The line break is conditional** — "do a line-break if too long", per the
/// issue. A name that fits the available width stays on one line:
///
/// ```
/// adam.eva
/// ```
///
/// and only a name that would overflow splits, namespace above and simple name
/// below:
///
/// ```
/// dl.lib.irod{100}.
/// x_rect{100}_centered
/// ```
///
/// **Why split at all, rather than ellipsize.** These names get long and the
/// panels that host this header are narrow (the node-data panel is 400px). On
/// one line the text ellipsizes at the *right* — truncating exactly the
/// informative half, since the simple name is the last segment of a qualified
/// name. Splitting keeps the namespace as visible context without letting it
/// push the name out of view. But it costs a row of height, so it is spent only
/// when it buys something: always splitting made a two-word name like
/// `adam.eva` as tall as a deep library path.
///
/// **The trailing dot belongs to the first line, with nothing after it.** The
/// two lines then concatenate to precisely the qualified name, so a selection
/// dragged across both yields something pasteable rather than something with a
/// stray space in the middle.
///
/// At the root there is no namespace at all, so there is nothing to split and
/// the header is always one line.
///
/// The name is **selectable** and carries a copy button (issue #359: a surface
/// that holds still gets real selection *plus* a ⧉; only transient surfaces
/// make do with a copy action alone). The ⧉ copies the whole qualified name in
/// one click, which is issue #307.
class QualifiedNameHeader extends StatelessWidget {
  const QualifiedNameHeader({
    super.key,
    required this.qualifiedName,
    required this.icon,
    this.copyTooltip = 'Copy qualified name',
    this.copyConfirmation = 'Name copied to clipboard',
  });

  /// The full dot-delimited name, e.g. `dl.lib.irod100.x_rect_centered`.
  final String qualifiedName;

  /// Leading icon identifying the kind of thing being named.
  final IconData icon;

  final String copyTooltip;
  final String copyConfirmation;

  /// Horizontal space `RenderEditable` keeps for the caret and can therefore
  /// not give to text: `_kCaretGap` (1.0) + the default `cursorWidth` (2.0),
  /// plus a pixel against fractional-width rounding. Measuring against the raw
  /// constraint overshoots by this much and clips the tail of the name.
  static const double _caretMargin = 4.0;

  /// Whether [span] laid out as a single line stays within [maxWidth].
  ///
  /// **The probe must resolve exactly the style the widget will render with.**
  /// The spans here set only `fontSize`/`color`/`weight` and inherit the rest,
  /// so at render time they merge onto the ambient `DefaultTextStyle` — under
  /// the stock Material theme that is `bodyMedium`, which carries a
  /// `letterSpacing` of 0.25 and its own font family. A `TextPainter` given the
  /// bare spans inherits neither and measures a name several characters
  /// narrower than it draws, so a name that "fits" gets its tail clipped.
  /// Seeding the root span with `DefaultTextStyle.of(context).style` reproduces
  /// the merge that `SelectableText` performs internally; the accessibility
  /// bold-text override is folded in for the same reason.
  static bool _fitsOnOneLine(
      BuildContext context, TextSpan span, double maxWidth) {
    if (!maxWidth.isFinite) return true;
    var base = DefaultTextStyle.of(context).style;
    if (MediaQuery.boldTextOf(context)) {
      base = base.merge(const TextStyle(fontWeight: FontWeight.bold));
    }
    final painter = TextPainter(
      text: TextSpan(style: base, children: [span]),
      maxLines: 1,
      textDirection: Directionality.of(context),
      textScaler: MediaQuery.textScalerOf(context),
    )..layout(
        maxWidth: (maxWidth - _caretMargin).clamp(0.0, double.infinity),
      );
    final fits = !painter.didExceedMaxLines;
    painter.dispose();
    return fits;
  }

  @override
  Widget build(BuildContext context) {
    final namespace = getNamespace(qualifiedName);
    final simpleName = getSimpleName(qualifiedName);
    final namespaceStyle =
        AppTextStyles.small.copyWith(color: Colors.grey.shade700);
    final nameStyle =
        AppTextStyles.regular.copyWith(fontWeight: FontWeight.w600);

    // Both lines live in ONE text widget rather than a Column of two. A
    // `SelectableText` reserves editable-text metrics, so stacking two of them
    // cost ~95px of strip for a 14px font; one widget under a forced strut is
    // roughly half that. It also makes a drag across both lines a single
    // selection yielding exactly the qualified name.
    TextSpan spanFor({required bool split}) => TextSpan(
          children: [
            if (namespace.isNotEmpty)
              TextSpan(
                text: split ? '$namespace.\n' : '$namespace.',
                style: namespaceStyle,
              ),
            TextSpan(text: simpleName, style: nameStyle),
          ],
        );

    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 5),
      decoration: BoxDecoration(
        color: Colors.grey.shade200,
        border: Border(
          bottom: BorderSide(color: Colors.grey.shade400, width: 1),
        ),
      ),
      child: Row(
        children: [
          Icon(icon, size: 18),
          const SizedBox(width: 8),
          Expanded(
            child: LayoutBuilder(
              builder: (context, constraints) {
                // Spend the second row only when one row cannot hold the name.
                final split = namespace.isNotEmpty &&
                    !_fitsOnOneLine(
                        context, spanFor(split: false), constraints.maxWidth);
                return SelectableText.rich(
                  spanFor(split: split),
                  strutStyle: const StrutStyle(
                    fontSize: 14,
                    height: 1.15,
                    forceStrutHeight: true,
                  ),
                  // When split, the namespace may still need a row of its own
                  // to wrap into before it has to ellipsize; when not, one row
                  // is by definition enough.
                  maxLines: split ? 3 : 1,
                );
              },
            ),
          ),
          const SizedBox(width: 8),
          CopyTextButton(
            text: qualifiedName,
            tooltip: copyTooltip,
            confirmation: copyConfirmation,
            size: 16,
          ),
        ],
      ),
    );
  }
}

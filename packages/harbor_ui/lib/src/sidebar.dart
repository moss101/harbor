import 'package:flutter/material.dart';

import 'foundation.dart';
import 'navigation.dart' show HarborDestination;
import 'tokens.dart';

/// The answer-first sidebar: a slim column with the product mark, one
/// primary action (a new thread), a few destinations, a scrolling section
/// (recent threads) and a footer for global controls.
///
/// Open ([HarborLayout.sidebar]) it shows labels and the section;
/// collapsed ([HarborLayout.sidebarCollapsed]) it keeps only icons, with
/// labels as tooltips, and hides the section. Phones show the open form
/// inside a drawer. Every row is a focusable semantic button with
/// `selected` state, and the layout mirrors in RTL.
class HarborSidebar extends StatelessWidget {
  const HarborSidebar({
    super.key,
    required this.destinations,
    required this.selectedIndex,
    required this.onSelected,
    this.collapsed = false,
    this.onToggleCollapsed,
    this.toggleTooltip,
    this.primaryAction,
    this.section,
    this.footer,
    this.shortcutHint,
    this.width,
  });

  final List<HarborDestination> destinations;

  /// Index into [destinations], or -1 when none is current (e.g. a
  /// secondary surface reached from the footer).
  final int selectedIndex;
  final ValueChanged<int> onSelected;
  final bool collapsed;

  /// Shows the collapse/expand control in the header when set.
  final VoidCallback? onToggleCollapsed;
  final String? toggleTooltip;

  /// The one prominent action under the header ("New thread").
  final HarborSidebarAction? primaryAction;

  /// Scrolling content under the destinations (open form only).
  final Widget? section;

  /// Pinned at the bottom (trust chip, menus).
  final Widget? footer;

  /// Shortcut label per destination index for tooltips (e.g. "⌘1").
  final String Function(int index)? shortcutHint;

  /// Overrides the width (a drawer passes its own).
  final double? width;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final w = width ??
        (collapsed ? HarborLayout.sidebarCollapsed : HarborLayout.sidebar);
    final hPad = collapsed ? HarborSpace.s2 : HarborSpace.s3;
    final toggle = onToggleCollapsed == null
        ? null
        : IconButton(
            key: const ValueKey('sidebar-toggle'),
            tooltip: toggleTooltip,
            onPressed: onToggleCollapsed,
            icon: Icon(
              collapsed ? Icons.menu : Icons.menu_open,
              size: 20,
              color: t.colors.inkMuted,
            ),
          );
    final header = collapsed
        ? Column(mainAxisSize: MainAxisSize.min, children: [
            const HarborWordmark(compact: true),
            if (toggle != null) ...[
              const SizedBox(height: HarborSpace.s2),
              toggle,
            ],
          ])
        : Row(children: [
            const Expanded(
              child: Align(
                alignment: AlignmentDirectional.centerStart,
                child: HarborWordmark(),
              ),
            ),
            if (toggle != null) toggle,
          ]);
    return Semantics(
      container: true,
      explicitChildNodes: true,
      child: Container(
        width: w,
        decoration: BoxDecoration(
          color: t.colors.surface,
          border: BorderDirectional(end: BorderSide(color: t.colors.border)),
        ),
        child: SafeArea(
          right: false,
          left: false,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Padding(
                padding: EdgeInsetsDirectional.fromSTEB(
                    collapsed ? 0 : HarborSpace.s4,
                    HarborSpace.s4,
                    collapsed ? 0 : HarborSpace.s2,
                    HarborSpace.s3),
                child: header,
              ),
              if (primaryAction case final a?)
                Padding(
                  padding: EdgeInsets.fromLTRB(hPad, 0, hPad, HarborSpace.s2),
                  child: _PrimaryAction(action: a, collapsed: collapsed),
                ),
              for (final (i, d) in destinations.indexed)
                Padding(
                  padding: EdgeInsets.symmetric(horizontal: hPad),
                  child: HarborSidebarRow(
                    icon: i == selectedIndex
                        ? (d.selectedIcon ?? d.icon)
                        : d.icon,
                    label: d.label,
                    selected: i == selectedIndex,
                    collapsed: collapsed,
                    shortcut: shortcutHint?.call(i),
                    onTap: () => onSelected(i),
                  ),
                ),
              Expanded(
                child: collapsed || section == null
                    ? const SizedBox.shrink()
                    : Padding(
                        padding:
                            EdgeInsets.fromLTRB(hPad, HarborSpace.s3, hPad, 0),
                        child: section,
                      ),
              ),
              if (footer != null)
                Padding(
                  padding: EdgeInsets.fromLTRB(
                      hPad, HarborSpace.s2, hPad, HarborSpace.s3),
                  child: footer,
                ),
            ],
          ),
        ),
      ),
    );
  }
}

/// The sidebar's one prominent action.
class HarborSidebarAction {
  const HarborSidebarAction({
    required this.label,
    required this.icon,
    required this.onPressed,
    this.key,
    this.shortcut,
  });
  final String label;
  final IconData icon;
  final VoidCallback? onPressed;
  final Key? key;
  final String? shortcut;
}

class _PrimaryAction extends StatelessWidget {
  const _PrimaryAction({required this.action, required this.collapsed});
  final HarborSidebarAction action;
  final bool collapsed;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    if (collapsed) {
      return Center(
        child: IconButton.filledTonal(
          key: action.key,
          tooltip: action.label,
          onPressed: action.onPressed,
          icon: Icon(action.icon, size: 20),
        ),
      );
    }
    return OutlinedButton(
      key: action.key,
      onPressed: action.onPressed,
      style: OutlinedButton.styleFrom(
        minimumSize: const Size.fromHeight(HarborLayout.touchTarget),
        padding: const EdgeInsets.symmetric(horizontal: HarborSpace.s3),
        shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(HarborRadius.pill)),
        side: BorderSide(color: t.colors.border),
        backgroundColor: t.colors.canvas,
        foregroundColor: t.colors.ink,
        alignment: AlignmentDirectional.centerStart,
      ),
      child: Row(children: [
        Icon(action.icon, size: 18, color: t.colors.brand),
        const SizedBox(width: HarborSpace.s2),
        Expanded(
          child: Text(action.label,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: t.text.bodyStrongOf(t.colors.ink)),
        ),
        if (action.shortcut case final s? when s.isNotEmpty)
          Text(s, style: t.text.captionOf(t.colors.inkMuted)),
      ]),
    );
  }
}

/// One sidebar row: icon + label (open) or icon with tooltip (collapsed).
/// Also used for the rows of the sidebar's section (recent threads).
class HarborSidebarRow extends StatelessWidget {
  const HarborSidebarRow({
    super.key,
    required this.label,
    required this.onTap,
    this.icon,
    this.selected = false,
    this.collapsed = false,
    this.shortcut,
    this.dense = false,
    this.trailing,
  });
  final String label;
  final VoidCallback? onTap;
  final IconData? icon;
  final bool selected;
  final bool collapsed;
  final String? shortcut;

  /// Thread rows: smaller type, shorter target (still >= 36 px).
  final bool dense;
  final Widget? trailing;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final c = t.colors;
    final iconWidget = icon == null
        ? null
        : Icon(icon,
            size: dense ? 16 : 20, color: selected ? c.brand : c.inkMuted);
    final content = collapsed
        ? Center(child: iconWidget)
        : Row(children: [
            if (iconWidget != null) ...[
              iconWidget,
              const SizedBox(width: HarborSpace.s3),
            ],
            Expanded(
              child: Text(label,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: dense
                      ? t.text.smallOf(selected ? c.ink : c.inkMuted).copyWith(
                          fontWeight: selected ? FontWeight.w600 : null)
                      : (selected
                          ? t.text.bodyStrongOf(c.ink)
                          : t.text.bodyOf(c.ink))),
            ),
            if (trailing != null)
              trailing!
            else if (shortcut != null && shortcut!.isNotEmpty)
              Text(shortcut!, style: t.text.captionOf(c.inkMuted)),
          ]);
    final tooltip = !collapsed
        ? ''
        : (shortcut == null || shortcut!.isEmpty)
            ? label
            : '$label · $shortcut';
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 1),
      child: Semantics(
        button: true,
        selected: selected,
        label: label,
        child: ExcludeSemantics(
          child: Tooltip(
            message: tooltip,
            waitDuration: const Duration(milliseconds: 500),
            // A thread row marks the open thread with a neutral wash, so
            // only the current destination carries the brand tint.
            child: Material(
              color: !selected
                  ? Colors.transparent
                  : (dense ? c.pressed : c.brandSoft),
              borderRadius: BorderRadius.circular(HarborRadius.sm),
              child: InkWell(
                onTap: onTap,
                borderRadius: BorderRadius.circular(HarborRadius.sm),
                hoverColor: c.hover,
                focusColor: c.focusRing.withValues(alpha: 0.18),
                child: ConstrainedBox(
                  constraints: BoxConstraints(
                      minHeight: dense ? 36 : HarborLayout.touchTarget),
                  child: Padding(
                    padding: EdgeInsets.symmetric(
                        horizontal: collapsed ? 0 : HarborSpace.s3),
                    child: content,
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

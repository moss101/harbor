import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../main.dart';
import '../services/harbor_service.dart';
import '../widgets/trust.dart';
import 'command_palette.dart';
import 'harbor_lens.dart';
import 'keyboard.dart';
import 'surface_stack.dart';

export 'harbor_lens.dart' show HarborLens;

/// The answer-first product shell. Asking is the main path; everything
/// else steps aside:
/// - <=599 (phones): a slim top bar (menu · mark · new thread · Trust
///   chip) over the canvas; the sidebar opens as a drawer; the Lens is a
///   modal sheet. No bottom navigation.
/// - 600+: a slim sidebar at the start edge — new thread, Home · Work ·
///   Library, recent threads, and a footer with the Trust chip and a More
///   menu (Models · Skills · Activity · Settings). It starts collapsed to
///   icons below 1180 and open from 1180, and the person can toggle it.
///   The Lens is an end drawer, docked only on 1280+ when the person
///   turns docking on in Settings.
///
/// Desktop keeps the keyboard intents (⌘/Ctrl+1…7, ⌘K palette, ⌘L Lens,
/// ⌘O open file, ⌘, settings) and shortcut hints in sidebar tooltips.
class AdaptiveShell extends StatefulWidget {
  const AdaptiveShell({super.key, required this.state});
  final AppState state;

  @override
  State<AdaptiveShell> createState() => _AdaptiveShellState();
}

class _AdaptiveShellState extends State<AdaptiveShell> {
  final _scaffoldKey = GlobalKey<ScaffoldState>();

  AppState get state => widget.state;

  void _openLens(HarborWindowClass wc) {
    if (HarborBreakpoints.isCompact(wc)) {
      openHarborLens(context);
      return;
    }
    if (HarborBreakpoints.lensIsPersistent(wc) && state.lensDocked) {
      state.setLensDocked(false);
      return;
    }
    final scaffold = _scaffoldKey.currentState;
    if (scaffold == null) return;
    if (scaffold.isEndDrawerOpen) {
      scaffold.closeEndDrawer();
    } else {
      scaffold.openEndDrawer();
    }
  }

  @override
  Widget build(BuildContext context) {
    final width = MediaQuery.sizeOf(context).width;
    final wc = HarborBreakpoints.classify(width);
    final sp = HarborServiceProvider.of(context);
    final canvas = SurfaceStack(
      index: state.surfaceIndex,
      count: HarborSurface.values.length,
      builder: (i) => surfaceFor(i, state),
    );

    final body = HarborBreakpoints.isCompact(wc)
        ? _PhoneShell(
            scaffoldKey: _scaffoldKey,
            state: state,
            canvas: canvas,
            failed: sp.failed,
            onLens: () => _openLens(wc),
          )
        : _SidebarShell(
            scaffoldKey: _scaffoldKey,
            state: state,
            windowClass: wc,
            canvas: canvas,
            failed: sp.failed,
            onLens: () => _openLens(wc),
            onPalette: () => showCommandPalette(context, state),
          );

    return Actions(
      actions: <Type, Action<Intent>>{
        GoToSurfaceIntent: CallbackAction<GoToSurfaceIntent>(onInvoke: (i) {
          // Defensive: a digit past the contract maps to no surface.
          if (i.index >= HarborSurface.shortcutOrder.length) return null;
          state.selectSurface(HarborSurface.shortcutOrder[i.index].index);
          return null;
        }),
        CommandPaletteIntent: CallbackAction<CommandPaletteIntent>(
            onInvoke: (_) => showCommandPalette(context, state)),
        ToggleLensIntent:
            CallbackAction<ToggleLensIntent>(onInvoke: (_) => _openLens(wc)),
        OpenFileIntent: CallbackAction<OpenFileIntent>(
            onInvoke: (_) => state.requestOpenFile()),
        OpenSettingsIntent: CallbackAction<OpenSettingsIntent>(
            onInvoke: (_) => state.goTo(HarborSurface.settings)),
      },
      // A focused ancestor guarantees shortcuts resolve even before the
      // user tabs into a control.
      // System back (Android) walks back through visited surfaces to
      // Home, and only leaves the app from Home.
      child: PopScope(
        canPop: state.atRoot,
        onPopInvokedWithResult: (didPop, _) {
          if (!didPop) state.goBack();
        },
        child: Focus(autofocus: true, skipTraversal: true, child: body),
      ),
    );
  }
}

/// The sidebar's content, shared by the docked sidebar and the phone
/// drawer. [inDrawer] closes the drawer after any navigation.
class ShellSidebar extends StatelessWidget {
  const ShellSidebar({
    super.key,
    required this.state,
    required this.failed,
    required this.onLens,
    this.collapsed = false,
    this.inDrawer = false,
    this.onToggle,
    this.onPalette,
    this.width,
  });
  final AppState state;
  final bool failed;
  final VoidCallback onLens;
  final bool collapsed;
  final bool inDrawer;
  final VoidCallback? onToggle;
  final VoidCallback? onPalette;
  final double? width;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final all = harborDestinations(l10n);
    const primary = HarborSurface.primary;
    final desktop = harborHasKeyboardShortcuts && !inDrawer;

    void then(VoidCallback action) {
      if (inDrawer) Scaffold.maybeOf(context)?.closeDrawer();
      action();
    }

    final footer = _SidebarFooter(
      state: state,
      collapsed: collapsed,
      failed: failed,
      onLens: () => then(onLens),
      onPalette: desktop ? onPalette : null,
      onNavigate: (s) => then(() => state.goTo(s)),
    );

    return HarborSidebar(
      width: width,
      collapsed: collapsed,
      onToggleCollapsed: onToggle,
      toggleTooltip:
          collapsed ? l10n.shellSidebarExpand : l10n.shellSidebarCollapse,
      destinations: [for (final s in primary) all[s.index]],
      selectedIndex: primary.indexOf(state.surface),
      onSelected: (i) => then(() => state.goTo(primary[i])),
      shortcutHint: desktop
          ? (int i) {
              // Hints follow the ⌘1–7 contract, not the list position.
              final digit = HarborSurface.shortcutOrder.indexOf(primary[i]);
              return digit >= 0 ? harborShortcutHint('${digit + 1}') : '';
            }
          : null,
      primaryAction: HarborSidebarAction(
        key: const ValueKey('shell-new-thread'),
        label: l10n.shellNewThread,
        icon: Icons.add,
        onPressed: () => then(state.newThread),
      ),
      section: _RecentThreads(
        state: state,
        onOpen: (id) => then(() => state.openThread(id)),
        onManage: () => then(state.manageThreads),
      ),
      footer: footer,
    );
  }
}

/// Recent threads in the open sidebar: the latest first, the current one
/// marked, with the history dialog (rename, delete) one tap away.
class _RecentThreads extends StatelessWidget {
  const _RecentThreads(
      {required this.state, required this.onOpen, required this.onManage});
  final AppState state;
  final ValueChanged<String> onOpen;
  final VoidCallback onManage;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final threads = state.threads;
    final onHome = state.surface == HarborSurface.home;
    // One scrolling list (heading included), so a short window or large
    // text never overflows the sidebar.
    return ScrollConfiguration(
      behavior: ScrollConfiguration.of(context).copyWith(scrollbars: false),
      child: ListView(
        key: const ValueKey('shell-recent-threads'),
        padding: EdgeInsets.zero,
        children: [
          Padding(
            padding: const EdgeInsetsDirectional.fromSTEB(
                HarborSpace.s3, 0, HarborSpace.s1, HarborSpace.s1),
            child: Row(children: [
              Expanded(
                child: Text(l10n.shellRecentThreads,
                    style: t.text.captionOf(t.colors.inkMuted)),
              ),
              IconButton(
                key: const ValueKey('shell-manage-threads'),
                tooltip: l10n.shellManageThreads,
                visualDensity: VisualDensity.compact,
                iconSize: 18,
                onPressed: onManage,
                icon: Icon(Icons.history, color: t.colors.inkMuted),
              ),
            ]),
          ),
          if (threads.isEmpty)
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: HarborSpace.s3),
              child: Text(l10n.shellNoThreads,
                  style: t.text.smallOf(t.colors.inkMuted)),
            )
          else
            for (final th in threads)
              HarborSidebarRow(
                key: ValueKey('shell-thread-${th['thread_id']}'),
                dense: true,
                // Thread titles are data (the first question).
                label: _title(th, l10n),
                selected: onHome && th['thread_id'] == state.currentThreadId,
                onTap: () => onOpen(th['thread_id'] as String),
              ),
        ],
      ),
    );
  }

  static String _title(Map<String, dynamic> th, AppLocalizations l10n) {
    final title = ((th['title'] as String?) ?? '').trim();
    return title.isEmpty ? l10n.askConversationUntitled : title;
  }
}

/// The sidebar footer: the Trust chip (opens the Lens), the command
/// palette on desktop, and More — the surfaces out of the main path.
class _SidebarFooter extends StatelessWidget {
  const _SidebarFooter({
    required this.state,
    required this.collapsed,
    required this.failed,
    required this.onLens,
    required this.onPalette,
    required this.onNavigate,
  });
  final AppState state;
  final bool collapsed;
  final bool failed;
  final VoidCallback onLens;
  final VoidCallback? onPalette;
  final ValueChanged<HarborSurface> onNavigate;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    // Large text: the chip keeps its icon so the footer row still fits.
    final bigText = MediaQuery.textScalerOf(context).scale(10) > 13;
    final trust = TrustChipLive(
        onTap: onLens, iconOnly: collapsed || bigText, failed: failed);
    final palette = onPalette == null
        ? null
        : IconButton(
            tooltip:
                '${l10n.commandPaletteTooltip} · ${harborShortcutHint('K')}',
            onPressed: onPalette,
            icon: const Icon(Icons.keyboard_command_key, size: 20),
          );
    final more = ShellMoreMenu(state: state, onNavigate: onNavigate);
    if (collapsed) {
      return Column(mainAxisSize: MainAxisSize.min, children: [
        if (palette != null) palette,
        more,
        const SizedBox(height: HarborSpace.s1),
        trust,
      ]);
    }
    return Row(children: [
      trust,
      const Spacer(),
      if (palette != null) palette,
      more,
    ]);
  }
}

/// More: Models · Skills · Activity · Settings, as a menu.
class ShellMoreMenu extends StatelessWidget {
  const ShellMoreMenu(
      {super.key, required this.state, required this.onNavigate});
  final AppState state;
  final ValueChanged<HarborSurface> onNavigate;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final all = harborDestinations(l10n);
    final onSecondary = HarborSurface.secondary.contains(state.surface);
    return MenuAnchor(
      builder: (context, controller, _) => IconButton(
        key: const ValueKey('shell-more'),
        tooltip: l10n.navMore,
        isSelected: onSecondary,
        onPressed: () =>
            controller.isOpen ? controller.close() : controller.open(),
        icon: const Icon(Icons.more_horiz),
        selectedIcon: Icon(Icons.more_horiz, color: t.colors.brand),
      ),
      menuChildren: [
        for (final s in HarborSurface.secondary)
          MenuItemButton(
            key: ValueKey('shell-more-${s.name}'),
            leadingIcon: Icon(
              s == state.surface
                  ? (all[s.index].selectedIcon ?? all[s.index].icon)
                  : all[s.index].icon,
              size: 20,
              color: s == state.surface ? t.colors.brand : t.colors.inkMuted,
            ),
            trailingIcon: s == state.surface
                ? Icon(Icons.check, size: 18, color: t.colors.brand)
                : null,
            onPressed: () => onNavigate(s),
            child: Text(all[s.index].label),
          ),
      ],
    );
  }
}

/// Phone shell: slim top bar, canvas, the sidebar as a drawer.
class _PhoneShell extends StatelessWidget {
  const _PhoneShell({
    required this.scaffoldKey,
    required this.state,
    required this.canvas,
    required this.failed,
    required this.onLens,
  });

  final GlobalKey<ScaffoldState> scaffoldKey;
  final AppState state;
  final Widget canvas;
  final bool failed;
  final VoidCallback onLens;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final current = state.surface;
    final destinations = harborDestinations(l10n);
    final drawerWidth =
        math.min(320.0, MediaQuery.sizeOf(context).width * 0.86);

    return Scaffold(
      key: scaffoldKey,
      drawer: Drawer(
        width: drawerWidth,
        backgroundColor: t.colors.surface,
        shape: const RoundedRectangleBorder(),
        child: ShellSidebar(
          state: state,
          failed: failed,
          onLens: onLens,
          inDrawer: true,
          width: drawerWidth,
        ),
      ),
      appBar: AppBar(
        titleSpacing: 0,
        // Narrow phones at large text keep the mark's glyph only, so the
        // bar's actions keep their room.
        title: current == HarborSurface.home
            ? HarborWordmark(
                compact: MediaQuery.sizeOf(context).width < 360 ||
                    MediaQuery.textScalerOf(context).scale(10) > 15)
            : Text(destinations[current.index].label),
        actions: [
          IconButton(
            key: const ValueKey('shell-new-thread-bar'),
            tooltip: l10n.shellNewThread,
            onPressed: state.newThread,
            icon: const Icon(Icons.add_comment_outlined),
          ),
          Padding(
            padding: const EdgeInsetsDirectional.only(end: HarborSpace.s3),
            child: TrustChipLive(onTap: onLens, failed: failed),
          ),
        ],
      ),
      body: SafeArea(top: false, bottom: false, child: canvas),
      backgroundColor: t.colors.canvas,
    );
  }
}

/// Tablet/desktop shell: sidebar + canvas (+ docked Lens when chosen).
class _SidebarShell extends StatelessWidget {
  const _SidebarShell({
    required this.scaffoldKey,
    required this.state,
    required this.windowClass,
    required this.canvas,
    required this.failed,
    required this.onLens,
    required this.onPalette,
  });

  final GlobalKey<ScaffoldState> scaffoldKey;
  final AppState state;
  final HarborWindowClass windowClass;
  final Widget canvas;
  final bool failed;
  final VoidCallback onLens;
  final VoidCallback onPalette;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final wc = windowClass;
    final open =
        state.sidebarOpen ?? HarborBreakpoints.sidebarOpenByDefault(wc);
    final docked = HarborBreakpoints.lensIsPersistent(wc) && state.lensDocked;

    final lensDrawer = docked
        ? null
        : Drawer(
            width: HarborLayout.desktopLens,
            backgroundColor: t.colors.surfaceRaised,
            shape: const RoundedRectangleBorder(),
            child: HarborLens(
              onClose: () => scaffoldKey.currentState?.closeEndDrawer(),
            ),
          );

    return Scaffold(
      key: scaffoldKey,
      endDrawer: lensDrawer,
      endDrawerEnableOpenDragGesture: false,
      backgroundColor: t.colors.canvas,
      body: Row(
        children: [
          ShellSidebar(
            state: state,
            failed: failed,
            onLens: onLens,
            collapsed: !open,
            onToggle: () => state.setSidebarOpen(!open),
            onPalette: onPalette,
          ),
          Expanded(child: SafeArea(child: canvas)),
          if (docked) ...[
            VerticalDivider(width: 1, color: t.colors.border),
            const SizedBox(
                width: HarborLayout.desktopLens, child: HarborLens()),
          ],
        ],
      ),
    );
  }
}

/// Opens the Harbor Lens as a transient sheet (compact class).
void openHarborLens(BuildContext context) {
  showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    useSafeArea: true,
    builder: (sheetContext) => SizedBox(
      height: MediaQuery.sizeOf(sheetContext).height * 0.85,
      child: HarborLens(onClose: () => Navigator.of(sheetContext).pop()),
    ),
  );
}

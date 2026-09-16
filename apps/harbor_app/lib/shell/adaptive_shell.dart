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

/// The adaptive product shell (UI authority §4/§20):
/// - <=599: top bar (title + Trust chip), five-item bottom navigation
///   (Home · Ask · Work · Models · More), Lens as a modal sheet
/// - 600-1023: icon + label side rail, Lens as an end drawer
/// - 1024-1179: collapsed 72px rail, Lens drawer; canvas >= 640
/// - 1180-1279: full 220px rail, Lens drawer
/// - 1280+: 220px rail + docked 320px Lens (toggleable), canvas >= 640
///
/// Desktop adds keyboard intents (⌘/Ctrl+1…9, ⌘K palette, ⌘L Lens, ⌘O
/// open file, ⌘, settings) and shortcut hints in rail tooltips.
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
    if (HarborBreakpoints.lensIsPersistent(wc)) {
      state.setLensDocked(!state.lensDocked);
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
    final l10n = AppLocalizations.of(context)!;
    final width = MediaQuery.sizeOf(context).width;
    final wc = HarborBreakpoints.classify(width);
    final destinations = harborDestinations(l10n);
    final sp = HarborServiceProvider.of(context);
    final canvas = SurfaceStack(
      index: state.surfaceIndex,
      count: HarborSurface.values.length,
      builder: (i) => surfaceFor(i, state),
    );

    final body = switch (wc) {
      HarborWindowClass.compact => _CompactShell(
          scaffoldKey: _scaffoldKey,
          state: state,
          destinations: destinations,
          canvas: canvas,
          failed: sp.failed,
          onLens: () => _openLens(wc),
        ),
      _ => _RailShell(
          scaffoldKey: _scaffoldKey,
          state: state,
          windowClass: wc,
          destinations: destinations,
          canvas: canvas,
          failed: sp.failed,
          onLens: () => _openLens(wc),
          onPalette: () => showCommandPalette(context, state),
        ),
    };

    return Actions(
      actions: <Type, Action<Intent>>{
        GoToSurfaceIntent: CallbackAction<GoToSurfaceIntent>(
            onInvoke: (i) => state.selectSurface(i.index)),
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
      child: Focus(autofocus: true, skipTraversal: true, child: body),
    );
  }
}

/// Phone-class shell: top bar, canvas, five-item bottom navigation.
class _CompactShell extends StatelessWidget {
  const _CompactShell({
    required this.scaffoldKey,
    required this.state,
    required this.destinations,
    required this.canvas,
    required this.failed,
    required this.onLens,
  });

  final GlobalKey<ScaffoldState> scaffoldKey;
  final AppState state;
  final List<HarborDestination> destinations;
  final Widget canvas;
  final bool failed;
  final VoidCallback onLens;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final current = state.surface;
    const primary = HarborSurface.primary;
    final onPrimary = primary.contains(current);
    final selectedIndex = onPrimary ? primary.indexOf(current) : primary.length;
    // "More" keeps its label and simply reads as selected while a
    // secondary surface is active; the app bar names that surface.
    final moreDestination = HarborDestination(l10n.navMore, Icons.apps_outlined,
        selectedIcon: Icons.apps);

    return Scaffold(
      key: scaffoldKey,
      appBar: AppBar(
        title: current == HarborSurface.home
            ? const HarborWordmark()
            : Text(destinations[current.index].label),
        actions: [
          Padding(
            padding: const EdgeInsetsDirectional.only(end: HarborSpace.s3),
            child: TrustChipLive(onTap: onLens, failed: failed),
          ),
        ],
      ),
      body: SafeArea(top: false, bottom: false, child: canvas),
      bottomNavigationBar: NavigationBar(
        selectedIndex: selectedIndex,
        onDestinationSelected: (i) {
          if (i < primary.length) {
            state.goTo(primary[i]);
          } else {
            _showMoreSheet(context);
          }
        },
        destinations: [
          for (final s in primary)
            NavigationDestination(
              icon: Icon(destinations[s.index].icon),
              selectedIcon: Icon(destinations[s.index].selectedIcon ??
                  destinations[s.index].icon),
              label: destinations[s.index].label,
              tooltip: '',
            ),
          NavigationDestination(
            icon: Icon(moreDestination.icon),
            selectedIcon:
                Icon(moreDestination.selectedIcon ?? moreDestination.icon),
            label: moreDestination.label,
            tooltip: '',
          ),
        ],
      ),
      backgroundColor: t.colors.canvas,
    );
  }

  Future<void> _showMoreSheet(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    return showModalBottomSheet<void>(
      context: context,
      useSafeArea: true,
      // Tall enough for all five destinations at 200% text on a 320px
      // phone; the list still scrolls beyond that.
      isScrollControlled: true,
      constraints:
          BoxConstraints(maxHeight: MediaQuery.sizeOf(context).height * 0.85),
      builder: (sheetContext) => SafeArea(
        top: false,
        child: ListView(
          shrinkWrap: true,
          padding: const EdgeInsets.fromLTRB(
              HarborSpace.s3, 0, HarborSpace.s3, HarborSpace.s4),
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(HarborSpace.s3, HarborSpace.s1,
                  HarborSpace.s3, HarborSpace.s3),
              child: Text(l10n.navMoreTitle,
                  style: t.text.captionOf(t.colors.inkMuted)),
            ),
            for (final s in HarborSurface.secondary)
              HarborListRow(
                leading: Icon(destinations[s.index].icon),
                title: Text(destinations[s.index].label),
                selected: s == state.surface,
                trailing: s == state.surface
                    ? Icon(Icons.check, size: 18, color: t.colors.brand)
                    : null,
                onTap: () {
                  Navigator.of(sheetContext).pop();
                  state.goTo(s);
                },
              ),
          ],
        ),
      ),
    );
  }
}

/// Tablet/desktop shell: rail + canvas (+ docked Lens on wide windows).
class _RailShell extends StatelessWidget {
  const _RailShell({
    required this.scaffoldKey,
    required this.state,
    required this.windowClass,
    required this.destinations,
    required this.canvas,
    required this.failed,
    required this.onLens,
    required this.onPalette,
  });

  final GlobalKey<ScaffoldState> scaffoldKey;
  final AppState state;
  final HarborWindowClass windowClass;
  final List<HarborDestination> destinations;
  final Widget canvas;
  final bool failed;
  final VoidCallback onLens;
  final VoidCallback onPalette;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final wc = windowClass;
    final medium = wc == HarborWindowClass.medium;
    final railWidth = medium ? 84.0 : HarborBreakpoints.railWidth(wc);
    final mode = medium
        ? HarborRailMode.medium
        : HarborBreakpoints.railIsExtended(wc)
            ? HarborRailMode.extended
            : HarborRailMode.collapsed;
    final extended = mode == HarborRailMode.extended;
    final docked = HarborBreakpoints.lensIsPersistent(wc) && state.lensDocked;
    final desktop = harborHasKeyboardShortcuts;

    final footer = Column(
      crossAxisAlignment:
          extended ? CrossAxisAlignment.stretch : CrossAxisAlignment.center,
      children: [
        Align(
          alignment: AlignmentDirectional.centerStart,
          child:
              TrustChipLive(onTap: onLens, iconOnly: !extended, failed: failed),
        ),
        const SizedBox(height: HarborSpace.s2),
        Wrap(
          alignment: extended ? WrapAlignment.start : WrapAlignment.center,
          children: [
            IconButton(
              tooltip: desktop
                  ? '${l10n.lensToggleTooltip} · ${harborShortcutHint('L')}'
                  : l10n.lensToggleTooltip,
              onPressed: onLens,
              isSelected: docked,
              icon: Icon(
                  docked ? Icons.view_sidebar : Icons.view_sidebar_outlined),
            ),
            if (desktop)
              IconButton(
                tooltip:
                    '${l10n.commandPaletteTooltip} · ${harborShortcutHint('K')}',
                onPressed: onPalette,
                icon: const Icon(Icons.keyboard_command_key),
              ),
          ],
        ),
      ],
    );

    final rail = HarborRail(
      width: railWidth,
      mode: mode,
      destinations: destinations,
      selectedIndex: state.surfaceIndex,
      onSelected: state.selectSurface,
      shortcutHint: desktop ? (i) => harborShortcutHint('${i + 1}') : null,
      footer: footer,
    );

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
      body: Row(
        children: [
          rail,
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

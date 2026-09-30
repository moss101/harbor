import 'package:flutter/material.dart';
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../main.dart';

/// The office suite shell: Work and Settings. Compact widths use a bottom
/// navigation bar; medium/wide use a side rail (the same breakpoint rules
/// as the main app's shell, minus the AI surfaces).
class OfficeShell extends StatelessWidget {
  const OfficeShell({super.key, required this.state});
  final AppState state;

  @override
  Widget build(BuildContext context) {
    final wc = HarborBreakpoints.of(context);
    final compact = HarborBreakpoints.isCompact(wc);
    final destinations = officeDestinations(AppLocalizations.of(context)!);

    final body = IndexedStack(
      index: state.surfaceIndex,
      children: [
        for (final s in OfficeSurface.values)
          officeSurfaceFor(s.index, state),
      ],
    );

    // SafeArea: the surface headers must never paint under the status
    // bar (found on the simulator — the subtitle sat behind the clock).
    return Scaffold(
      body: SafeArea(
        bottom: false,
        child: compact
            ? body
            : Row(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  HarborRail(
                    destinations: destinations,
                    selectedIndex: state.surfaceIndex,
                    onSelected: state.selectSurface,
                  ),
                  Expanded(child: body),
                ],
              ),
      ),
      bottomNavigationBar: compact
          ? NavigationBar(
              selectedIndex: state.surfaceIndex,
              onDestinationSelected: state.selectSurface,
              destinations: [
                for (final d in destinations)
                  NavigationDestination(
                    icon: Icon(d.icon),
                    selectedIcon: d.selectedIcon == null
                        ? Icon(d.icon)
                        : Icon(d.selectedIcon!),
                    label: d.label,
                  ),
              ],
            )
          : null,
    );
  }
}

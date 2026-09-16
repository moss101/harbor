import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../main.dart';
import '../services/harbor_service.dart';
import 'keyboard.dart';

/// One palette entry: a surface, an action or a skill. Only real
/// navigation and real actions are listed — nothing is decorative.
class _Command {
  const _Command({
    required this.section,
    required this.title,
    required this.icon,
    required this.run,
    this.subtitle,
    this.hint,
  });
  final String section;
  final String title;
  final String? subtitle;
  final IconData icon;
  final String? hint;
  final VoidCallback run;

  bool matches(String q) {
    if (q.isEmpty) return true;
    final needle = q.toLowerCase();
    return title.toLowerCase().contains(needle) ||
        (subtitle?.toLowerCase().contains(needle) ?? false) ||
        section.toLowerCase().contains(needle);
  }
}

/// Desktop command palette (⌘K / Ctrl+K): surfaces, actions and skills.
Future<void> showCommandPalette(BuildContext context, AppState state) {
  final service = HarborServiceProvider.of(context).notifier;
  return showDialog<void>(
    context: context,
    barrierLabel: AppLocalizations.of(context)!.commandPaletteTooltip,
    builder: (_) => _CommandPalette(state: state, service: service),
  );
}

class _CommandPalette extends StatefulWidget {
  const _CommandPalette({required this.state, required this.service});
  final AppState state;
  final HarborService? service;

  @override
  State<_CommandPalette> createState() => _CommandPaletteState();
}

class _CommandPaletteState extends State<_CommandPalette> {
  final _controller = TextEditingController();
  final _scroll = ScrollController();
  int _highlight = 0;

  @override
  void dispose() {
    _controller.dispose();
    _scroll.dispose();
    super.dispose();
  }

  List<_Command> _all(AppLocalizations l10n) {
    final state = widget.state;
    final destinations = harborDestinations(l10n);
    final desktop = harborHasKeyboardShortcuts;
    final commands = <_Command>[
      for (final (i, d) in destinations.indexed)
        _Command(
          section: l10n.commandSectionSurfaces,
          title: l10n.commandGoTo(d.label),
          icon: d.icon,
          hint: desktop ? harborShortcutHint('${i + 1}') : null,
          run: () => state.selectSurface(i),
        ),
      _Command(
        section: l10n.commandSectionActions,
        title: l10n.openFile,
        icon: Icons.folder_open_outlined,
        hint: desktop ? harborShortcutHint('O') : null,
        run: state.requestOpenFile,
      ),
      _Command(
        section: l10n.commandSectionActions,
        title: l10n.addSources,
        subtitle: l10n.surfaceKnowledge,
        icon: Icons.note_add_outlined,
        run: () => state.goTo(HarborSurface.knowledge),
      ),
      _Command(
        section: l10n.commandSectionActions,
        title: l10n.commandToggleTheme,
        icon: Icons.brightness_6_outlined,
        run: () => state.setThemeMode(state.themeMode == ThemeMode.dark
            ? ThemeMode.light
            : ThemeMode.dark),
      ),
      _Command(
        section: l10n.commandSectionActions,
        title: l10n.commandSwitchLanguage,
        subtitle: state.locale.languageCode == 'ar'
            ? l10n.settingsEnglish
            : l10n.settingsArabic,
        icon: Icons.translate_outlined,
        run: () => state
            .setLocale(Locale(state.locale.languageCode == 'ar' ? 'en' : 'ar')),
      ),
      for (final (label, icon) in [
        (l10n.quickSummarize, Icons.description_outlined),
        (l10n.quickAnalyze, Icons.table_chart_outlined),
        (l10n.quickPresent, Icons.slideshow_outlined),
        (l10n.quickCompare, Icons.difference_outlined),
        (l10n.quickOrganize, Icons.folder_open_outlined),
        (l10n.quickResearch, Icons.travel_explore_outlined),
        (l10n.quickTranslate, Icons.translate_outlined),
      ])
        _Command(
          section: l10n.quickActionsHeading,
          title: label,
          icon: icon,
          run: () => state.composeOnHome(label),
        ),
      for (final s in widget.service?.skills ?? const [])
        _Command(
          section: l10n.surfaceSkills,
          title: s.title,
          subtitle: s.family,
          icon: Icons.construction_outlined,
          run: () => state.filterSkills(s.title),
        ),
    ];
    return commands;
  }

  void _run(_Command c) {
    Navigator.of(context).pop();
    c.run();
  }

  void _move(int delta, int count) {
    if (count == 0) return;
    setState(() => _highlight = (_highlight + delta) % count);
    if (_highlight < 0) _highlight += count;
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final query = _controller.text.trim();
    final results = _all(l10n).where((c) => c.matches(query)).toList();
    if (_highlight >= results.length) _highlight = 0;
    final size = MediaQuery.sizeOf(context);
    final compact = size.width < 600;
    return Dialog(
      alignment: compact ? Alignment.center : const Alignment(0, -0.6),
      insetPadding: EdgeInsets.all(compact ? HarborSpace.s4 : HarborSpace.s8),
      child: ConstrainedBox(
        constraints: BoxConstraints(
          maxWidth: 640,
          maxHeight: (size.height * 0.7).clamp(280, 560),
        ),
        child: CallbackShortcuts(
          bindings: {
            const SingleActivator(LogicalKeyboardKey.arrowDown): () =>
                _move(1, results.length),
            const SingleActivator(LogicalKeyboardKey.arrowUp): () =>
                _move(-1, results.length),
          },
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Padding(
                padding: const EdgeInsets.all(HarborSpace.s3),
                child: TextField(
                  controller: _controller,
                  autofocus: true,
                  onChanged: (_) => setState(() => _highlight = 0),
                  onSubmitted: (_) {
                    if (results.isNotEmpty) _run(results[_highlight]);
                  },
                  decoration: InputDecoration(
                    hintText: l10n.commandPaletteHint,
                    prefixIcon: const Icon(Icons.search),
                    suffixIcon: IconButton(
                      tooltip: l10n.closeAction,
                      icon: const Icon(Icons.close),
                      onPressed: () => Navigator.of(context).pop(),
                    ),
                  ),
                ),
              ),
              const Divider(height: 1),
              Flexible(
                child: results.isEmpty
                    ? Padding(
                        padding: const EdgeInsets.all(HarborSpace.s6),
                        child: Text(l10n.commandPaletteEmpty,
                            textAlign: TextAlign.center,
                            style: t.text.smallOf(t.colors.inkMuted)),
                      )
                    : ListView.builder(
                        controller: _scroll,
                        shrinkWrap: true,
                        padding: const EdgeInsets.symmetric(
                            vertical: HarborSpace.s2,
                            horizontal: HarborSpace.s2),
                        itemCount: results.length,
                        itemBuilder: (context, i) {
                          final c = results[i];
                          final showSection =
                              i == 0 || results[i - 1].section != c.section;
                          return Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              if (showSection)
                                Padding(
                                  padding: const EdgeInsets.fromLTRB(
                                      HarborSpace.s3,
                                      HarborSpace.s3,
                                      HarborSpace.s3,
                                      HarborSpace.s1),
                                  child: Text(c.section,
                                      style:
                                          t.text.captionOf(t.colors.inkMuted)),
                                ),
                              HarborListRow(
                                dense: true,
                                selected: i == _highlight,
                                leading: Icon(c.icon),
                                title: Text(c.title),
                                subtitle: c.subtitle == null
                                    ? null
                                    : Text(c.subtitle!),
                                trailing: c.hint == null
                                    ? null
                                    : Text(c.hint!,
                                        style: t.text
                                            .captionOf(t.colors.inkMuted)),
                                onTap: () => _run(c),
                              ),
                            ],
                          );
                        },
                      ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

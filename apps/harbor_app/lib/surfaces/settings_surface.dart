import 'dart:io';

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:harbor_ui/harbor_ui.dart';
import 'package:path_provider/path_provider.dart';

import '../build_info.dart';
import '../l10n/app_localizations.dart';
import '../main.dart';
import '../services/harbor_service.dart';
import '../shell/keyboard.dart';
import '../widgets/trust.dart';

/// Settings (UX-032/037/038): appearance, language, workspace privacy,
/// identity, keyboard shortcuts (desktop) and about — grouped in cards.
class SettingsSurface extends StatelessWidget {
  const SettingsSurface({super.key, required this.state});
  final AppState state;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    final wc = HarborBreakpoints.of(context);
    final desktop = harborHasKeyboardShortcuts;

    Widget section(String title, Widget child, {String? body}) => Padding(
          padding: const EdgeInsets.only(bottom: HarborSpace.s5),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              HarborSectionHeader(title: title, subtitle: body),
              child,
            ],
          ),
        );

    return Column(children: [
      HarborSurfaceHeader(
          title: l10n.surfaceSettings, subtitle: l10n.settingsSubtitle),
      Expanded(
        child: HarborPage(
          maxWidth: HarborLayout.readingMax + 80,
          children: [
            section(
              l10n.settingsTheme,
              HarborCard(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    SegmentedButton<ThemeMode>(
                      segments: [
                        ButtonSegment(
                            value: ThemeMode.system,
                            icon: const Icon(Icons.brightness_auto_outlined,
                                size: 16),
                            label: Text(l10n.settingsThemeSystem)),
                        ButtonSegment(
                            value: ThemeMode.light,
                            icon:
                                const Icon(Icons.light_mode_outlined, size: 16),
                            label: Text(l10n.settingsThemeLight)),
                        ButtonSegment(
                            value: ThemeMode.dark,
                            icon:
                                const Icon(Icons.dark_mode_outlined, size: 16),
                            label: Text(l10n.settingsThemeDark)),
                      ],
                      selected: {state.themeMode},
                      onSelectionChanged: (s) => state.setThemeMode(s.first),
                      showSelectedIcon: false,
                    ),
                    const SizedBox(height: HarborSpace.s3),
                    Text(l10n.settingsMotionNote,
                        style: t.text.captionOf(t.colors.inkMuted)),
                    if (HarborBreakpoints.lensIsPersistent(wc)) ...[
                      const SizedBox(height: HarborSpace.s2),
                      SwitchListTile(
                        contentPadding: EdgeInsets.zero,
                        title: Text(l10n.settingsLensDocked,
                            style: t.text.bodyOf(t.colors.ink)),
                        value: state.lensDocked,
                        onChanged: state.setLensDocked,
                      ),
                    ],
                  ],
                ),
              ),
            ),
            section(
              l10n.settingsLanguage,
              HarborCard(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    SegmentedButton<String>(
                      segments: [
                        ButtonSegment(
                            value: 'en', label: Text(l10n.settingsEnglish)),
                        ButtonSegment(
                            value: 'ar', label: Text(l10n.settingsArabic)),
                      ],
                      selected: {state.locale.languageCode},
                      onSelectionChanged: (s) =>
                          state.setLocale(Locale(s.first)),
                      showSelectedIcon: false,
                    ),
                    const SizedBox(height: HarborSpace.s3),
                    Text(l10n.settingsLanguageBody,
                        style: t.text.smallOf(t.colors.inkMuted)),
                  ],
                ),
              ),
            ),
            section(
              l10n.settingsPrivacy,
              Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                TrustPulseCard(service: service, failed: sp.failed),
                const SizedBox(height: HarborSpace.s3),
                HarborBanner(
                  tone: HarborBannerTone.info,
                  icon: Icons.shield_outlined,
                  title: l10n.settingsPrivacyBody,
                  body: l10n.trustLocalOnlyBody,
                ),
              ]),
            ),
            section(
              l10n.settingsDiagnostics,
              DiagnosticsCard(service: service),
              body: l10n.settingsDiagnosticsBody,
            ),
            section(
              l10n.settingsIdentity,
              HarborCard(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    if (service?.deviceId != null)
                      HarborKeyValue(
                        label: l10n.settingsIdentity,
                        value: service!.deviceId!,
                        identifier: true,
                        trailing: _CopyButton(value: service.deviceId!),
                      ),
                    if (service?.workspaceId != null)
                      HarborKeyValue(
                        label: l10n.settingsWorkspaceId,
                        value: service!.workspaceId!,
                        identifier: true,
                      ),
                    if (service?.deviceId == null &&
                        service?.workspaceId == null)
                      Text(sp.failed ? l10n.coreStartFailed : l10n.appStarting,
                          style: t.text.smallOf(t.colors.inkMuted)),
                  ],
                ),
              ),
            ),
            if (desktop)
              section(
                l10n.settingsShortcuts,
                HarborCard(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      _ShortcutRow(
                          label: l10n.settingsShortcutSurfaces,
                          keys:
                              '${harborShortcutHint('1')} … ${harborShortcutHint('9')}'),
                      _ShortcutRow(
                          label: l10n.settingsShortcutPalette,
                          keys: harborShortcutHint('K')),
                      _ShortcutRow(
                          label: l10n.commandToggleLens,
                          keys: harborShortcutHint('L')),
                      _ShortcutRow(
                          label: l10n.openFile, keys: harborShortcutHint('O')),
                      _ShortcutRow(
                          label: l10n.surfaceSettings,
                          keys: harborShortcutHint(',')),
                    ],
                  ),
                ),
              ),
            section(
              l10n.settingsAbout,
              HarborCard(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    const HarborWordmark(),
                    const SizedBox(height: HarborSpace.s2),
                    Text(l10n.appTagline,
                        style: t.text.captionOf(t.colors.inkMuted)),
                    const SizedBox(height: HarborSpace.s3),
                    HarborKeyValue(
                        label: l10n.settingsVersion,
                        value: '1.0.0',
                        identifier: true),
                    HarborKeyValue(
                      label: l10n.settingsCoreStatus,
                      value: sp.failed
                          ? l10n.settingsCoreDegraded
                          : l10n.settingsCoreLoaded,
                      trailing: StatusBadge(
                        semantic: sp.failed
                            ? ExecutionSemantic.danger
                            : ExecutionSemantic.local,
                        label: sp.failed ? 'OFFLINE' : 'LOCAL',
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ],
        ),
      ),
    ]);
  }
}

class _ShortcutRow extends StatelessWidget {
  const _ShortcutRow({required this.label, required this.keys});
  final String label;
  final String keys;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: HarborSpace.s1 + 2),
      child: Row(children: [
        Expanded(child: Text(label, style: t.text.smallOf(t.colors.ink))),
        Container(
          padding: const EdgeInsets.symmetric(
              horizontal: HarborSpace.s2, vertical: 2),
          decoration: ShapeDecoration(
            color: t.colors.surfaceRaised,
            shape: RoundedRectangleBorder(
              borderRadius: BorderRadius.circular(HarborRadius.sm / 2),
              side: BorderSide(color: t.colors.border),
            ),
          ),
          child: Directionality(
            textDirection: TextDirection.ltr,
            child:
                Text(keys, style: t.text.monoOf(t.colors.inkMuted, size: 11)),
          ),
        ),
      ]),
    );
  }
}

class _CopyButton extends StatelessWidget {
  const _CopyButton({required this.value});
  final String value;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    return IconButton(
      tooltip: l10n.copyAction,
      iconSize: 16,
      onPressed: () async {
        await Clipboard.setData(ClipboardData(text: value));
        if (!context.mounted) return;
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text(l10n.copiedMessage)));
      },
      icon: const Icon(Icons.copy_outlined),
    );
  }
}

/// Export diagnostics (production plan C1): the core writes a zip of
/// redacted crash/error records plus build, runtime and device facts —
/// never document content, knowledge chunks or prompts — to a location
/// the user picks. Nothing is uploaded; sharing is the user's act.
class DiagnosticsCard extends StatefulWidget {
  const DiagnosticsCard(
      {super.key, required this.service, this.resolveDestination});
  final HarborService? service;

  /// Replaces the platform save dialog (tests inject a fixed path).
  final Future<String?> Function(String suggestedName)? resolveDestination;

  @override
  State<DiagnosticsCard> createState() => _DiagnosticsCardState();
}

class _DiagnosticsCardState extends State<DiagnosticsCard> {
  bool _busy = false;
  Map<String, dynamic>? _report;
  String? _error;
  int? _recordCount;

  @override
  void initState() {
    super.initState();
    _loadCount();
  }

  Future<void> _loadCount() async {
    final s = widget.service;
    if (s == null) return;
    try {
      final list = await s.listDiagnostics(limit: 1);
      if (!mounted) return;
      setState(() => _recordCount = list['total'] as int?);
    } catch (_) {
      // The count is a courtesy; export still works.
    }
  }

  Future<String?> _defaultDestination(String suggestedName) async {
    if (Platform.isMacOS || Platform.isWindows || Platform.isLinux) {
      final location = await getSaveLocation(suggestedName: suggestedName);
      return location?.path;
    }
    final dir = await getApplicationDocumentsDirectory();
    return '${dir.path}${Platform.pathSeparator}$suggestedName';
  }

  Future<void> _export() async {
    final s = widget.service;
    if (s == null) return;
    final stamp = DateTime.now()
        .toUtc()
        .toIso8601String()
        .replaceAll(':', '')
        .split('.')
        .first;
    final resolve = widget.resolveDestination ?? _defaultDestination;
    final destination = await resolve('harbor-diagnostics-$stamp.zip');
    if (destination == null || !mounted) return;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final report = await s.exportDiagnostics(
          destination: destination, appVersion: harborAppVersion);
      if (!mounted) return;
      setState(() => _report = report);
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    return HarborCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(l10n.settingsDiagnosticsContains,
              style: t.text.smallOf(t.colors.inkMuted)),
          const SizedBox(height: HarborSpace.s3),
          // Wrap, not Row: the label and the button must survive 320 px at
          // 200 % text scale (accessibility gate).
          Wrap(
            alignment: WrapAlignment.spaceBetween,
            crossAxisAlignment: WrapCrossAlignment.center,
            spacing: HarborSpace.s3,
            runSpacing: HarborSpace.s2,
            children: [
              Text(
                _recordCount == null
                    ? ''
                    : l10n.settingsDiagnosticsRecords(_recordCount!),
                style: t.text.smallOf(t.colors.inkMuted),
              ),
              FilledButton.tonalIcon(
                key: const ValueKey('diagnostics-export'),
                onPressed: _busy || widget.service == null ? null : _export,
                icon: const Icon(Icons.outbox_outlined, size: 18),
                label: Text(l10n.settingsDiagnosticsExport),
              ),
            ],
          ),
          if (_report != null) ...[
            const SizedBox(height: HarborSpace.s3),
            HarborBanner(
              key: const ValueKey('diagnostics-exported'),
              tone: HarborBannerTone.info,
              title: l10n.settingsDiagnosticsExported,
              body: l10n.settingsDiagnosticsExportedBody(
                  _report!['path'] as String? ?? '',
                  (_report!['records'] as num?)?.toInt() ?? 0),
            ),
          ],
          if (_error != null) ...[
            const SizedBox(height: HarborSpace.s3),
            HarborBanner(
                tone: HarborBannerTone.danger,
                title: l10n.settingsDiagnosticsExportFailed,
                body: _error),
          ],
        ],
      ),
    );
  }
}

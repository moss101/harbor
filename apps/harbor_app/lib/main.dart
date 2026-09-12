import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:harbor_ui/harbor_ui.dart';

import 'dart:io';

import 'l10n/app_localizations.dart';
import 'services/harbor_service.dart';
import 'shell/adaptive_shell.dart';
import 'surfaces/surfaces.dart';

void main() {
  runApp(const HarborApp());
}

/// App-level UI state (language, theme, current surface). The Rust core
/// owns policy and runtime truth; this only mirrors presentation choices.
class AppState extends ChangeNotifier {
  Locale locale = const Locale('en');
  ThemeMode themeMode = ThemeMode.light;
  int surfaceIndex = 0;
  bool lensOpen = false;

  void setLocale(Locale l) {
    locale = l;
    notifyListeners();
  }

  void setThemeMode(ThemeMode m) {
    themeMode = m;
    notifyListeners();
  }

  void selectSurface(int i) {
    surfaceIndex = i;
    notifyListeners();
  }
}

class HarborApp extends StatefulWidget {
  const HarborApp({super.key, this.service});

  /// Injectable for tests; when null a real FFI service is created.
  final HarborService? service;

  @override
  State<HarborApp> createState() => _HarborAppState();
}

class _HarborAppState extends State<HarborApp> {
  final AppState _state = AppState();
  HarborService? _service;
  bool _serviceFailed = false;

  HarborService? get service => widget.service ?? _service;

  @override
  void initState() {
    super.initState();
    if (widget.service == null) {
      final lib =
          Platform.environment['HARBOR_FFI_LIB'] ?? 'libharbor_ffi.dylib';
      try {
        final dir = Directory.systemTemp.createTempSync('harbor-app-');
        _service = HarborService.open(libraryPath: lib, dataRoot: dir.path);
        _service!.refresh();
      } catch (_) {
        _serviceFailed = true;
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: _state,
      builder: (context, _) {
        final dark = _state.themeMode == ThemeMode.dark;
        final arabic = _state.locale.languageCode == 'ar';
        return MaterialApp(
          onGenerateTitle: (context) => 'Harbor',
          theme: harborThemeData(dark: false, arabic: arabic),
          darkTheme: harborThemeData(dark: true, arabic: arabic),
          themeMode: _state.themeMode,
          locale: _state.locale,
          localizationsDelegates: const [
            AppLocalizations.delegate,
            GlobalMaterialLocalizations.delegate,
            GlobalWidgetsLocalizations.delegate,
            GlobalCupertinoLocalizations.delegate,
          ],
          supportedLocales: const [Locale('en'), Locale('ar')],
          // RTL mirrors structure (flutter handles Directionality from
          // locale); technical data keeps intrinsic LTR via Directionality
          // wrappers at usage sites.
          builder: (context, child) => HarborTheme(
            colors: dark ? HarborColors.dark : HarborColors.light,
            text: HarborType(arabic: arabic),
            child: child ?? const SizedBox.shrink(),
          ),
          home: HarborServiceProvider(
            service: service,
            failed: _serviceFailed,
            child: AdaptiveShell(state: _state),
          ),
        );
      },
    );
  }
}

/// The nine product surfaces (goal §3): Home · Ask · Work · Agents ·
/// Models · Skills · Knowledge · Activity · Settings.
List<HarborDestination> harborDestinations(AppLocalizations l10n) => [
      HarborDestination(l10n.surfaceHome, Icons.home_outlined),
      HarborDestination(l10n.surfaceAsk, Icons.question_answer_outlined),
      HarborDestination(l10n.surfaceWork, Icons.description_outlined),
      HarborDestination(l10n.surfaceAgents, Icons.smart_toy_outlined),
      HarborDestination(l10n.surfaceModels, Icons.memory_outlined),
      HarborDestination(l10n.surfaceSkills, Icons.construction_outlined),
      HarborDestination(l10n.surfaceKnowledge, Icons.library_books_outlined),
      HarborDestination(l10n.surfaceActivity, Icons.timeline_outlined),
      HarborDestination(l10n.surfaceSettings, Icons.settings_outlined),
    ];

Widget surfaceFor(int index, AppState state) {
  switch (index) {
    case 0:
      return HomeSurface(state: state);
    case 1:
      return const AskSurface();
    case 2:
      return const WorkSurface();
    case 3:
      return const AgentsSurface();
    case 4:
      return const ModelsSurface();
    case 5:
      return const SkillsSurface();
    case 6:
      return const KnowledgeSurface();
    case 7:
      return const ActivitySurface();
    default:
      return SettingsSurface(state: state);
  }
}

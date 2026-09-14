import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:harbor_ui/harbor_ui.dart';
import 'package:path_provider/path_provider.dart';

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

  /// Injectable for tests; when null a real FFI service is created over
  /// the persistent application-support data root.
  final HarborService? service;

  @override
  State<HarborApp> createState() => _HarborAppState();
}

class _HarborAppState extends State<HarborApp> with WidgetsBindingObserver {
  final AppState _state = AppState();
  HarborService? _service;
  bool _serviceFailed = false;
  bool _initializing = false;

  HarborService? get service => widget.service ?? _service;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    if (widget.service == null) {
      _initializing = true;
      _bootstrap();
    }
  }

  /// Persistent storage: the workspace data root is the app-support
  /// directory (NOT a per-launch temp dir), so installed models, the
  /// knowledge index, keys and the durable run log all survive restarts.
  /// On Android the device root key comes from the embedding's
  /// AndroidKeyStore-backed channel; Apple platforms and Windows use the
  /// core's own keystore adapters (Keychain / DPAPI).
  Future<void> _bootstrap() async {
    HarborService? opened;
    try {
      final support = await getApplicationSupportDirectory();
      final dataRoot =
          Directory('${support.path}${Platform.pathSeparator}harbor-data');
      await dataRoot.create(recursive: true);
      String? deviceRootHex;
      if (Platform.isAndroid) {
        const channel = MethodChannel('dev.harbor.keystore');
        deviceRootHex = await channel.invokeMethod<String>('getRootKey');
      }
      opened = await HarborService.open(
        libraryPath: HarborBinding.defaultLibraryPath(),
        dataRoot: dataRoot.path,
        deviceRootHex: deviceRootHex,
      );
      await opened.refresh();
    } catch (_) {
      if (mounted) {
        setState(() {
          _serviceFailed = true;
          _initializing = false;
        });
      }
      return;
    }
    if (!mounted) {
      await opened.close();
      return;
    }
    setState(() {
      _service = opened;
      _initializing = false;
    });
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    // The view controller is going away for good: release the native
    // core (SQLite handles, loaded model memory) deterministically.
    if (state == AppLifecycleState.detached) {
      final s = _service;
      _service = null;
      s?.close();
    }
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    final s = _service;
    _service = null;
    s?.close();
    _state.dispose();
    super.dispose();
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
          home: _initializing
              ? const _BootingView()
              : HarborServiceProvider(
                  service: service,
                  failed: _serviceFailed,
                  child: AdaptiveShell(state: _state),
                ),
        );
      },
    );
  }
}

/// Shown while the persistent workspace opens on the worker isolate.
class _BootingView extends StatelessWidget {
  const _BootingView();

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    return Scaffold(
      body: Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const CircularProgressIndicator(),
            const SizedBox(height: HarborSpace.s4),
            Text(l10n.appStarting),
          ],
        ),
      ),
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

import 'dart:io';

import 'package:flutter/gestures.dart' show PointerDeviceKind;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:harbor_ui/harbor_ui.dart';
import 'package:path_provider/path_provider.dart';

import 'l10n/app_localizations.dart';
import 'services/diagnostics.dart';
import 'services/harbor_service.dart';
import 'services/preferences.dart';
import 'shell/office_shell.dart';
import 'surfaces/settings_surface.dart';
import 'surfaces/work_surface.dart';

void main() {
  WidgetsFlutterBinding.ensureInitialized();
  // Uncaught errors go to the core's encrypted diagnostics log (no
  // telemetry; the user exports it by hand from Settings).
  DiagnosticsSink.instance.install();
  // Edge-to-edge on Android (and a no-op elsewhere): the shell paints
  // behind the system bars and insets its own content.
  SystemChrome.setEnabledSystemUIMode(SystemUiMode.edgeToEdge);
  runApp(const HarborOfficeApp());
}

/// The two office surfaces: Work (documents, workbooks, decks, PDF) and
/// Settings. The suite is deliberately office-only — no models, ask,
/// agents or knowledge.
enum OfficeSurface { work, settings }

/// App-level UI state (language, theme, current surface). The Rust core
/// owns policy and runtime truth; this only mirrors presentation
/// choices, persisted through [PreferencesStore].
class AppState extends ChangeNotifier {
  AppState({PreferencesStore? store})
      : _store = store ?? MemoryPreferencesStore();

  PreferencesStore _store;
  HarborPreferences _prefs = const HarborPreferences();

  /// The real app learns its app-support path only during bootstrap, so
  /// the file store is attached after construction.
  void replaceStore(PreferencesStore store) => _store = store;

  int surfaceIndex = 0;

  /// Set by the desktop shortcut: the Work surface opens its picker on
  /// arrival.
  bool pendingOpenFile = false;

  Locale get locale => _prefs.locale;
  ThemeMode get themeMode => _prefs.themeMode;
  OfficeSurface get surface => OfficeSurface.values[surfaceIndex];

  Future<void> load() async {
    _prefs = await _store.load();
    notifyListeners();
  }

  void _update(HarborPreferences next) {
    if (next == _prefs) return;
    _prefs = next;
    notifyListeners();
    _store.save(next);
  }

  void setLocale(Locale l) => _update(_prefs.copyWith(locale: l));

  void setThemeMode(ThemeMode m) => _update(_prefs.copyWith(themeMode: m));

  void selectSurface(int i) {
    if (i == surfaceIndex) return;
    surfaceIndex = i;
    notifyListeners();
  }

  void goTo(OfficeSurface s) => selectSurface(s.index);

  /// Navigate to Work and ask it to open the file picker.
  void requestOpenFile() {
    pendingOpenFile = true;
    surfaceIndex = OfficeSurface.work.index;
    notifyListeners();
  }
}

/// Inherited access to [AppState] so surfaces can navigate without
/// constructor plumbing (and still render standalone in tests).
class AppStateScope extends InheritedNotifier<AppState> {
  const AppStateScope(
      {super.key, required AppState state, required super.child})
      : super(notifier: state);

  static AppState? maybeOf(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<AppStateScope>()?.notifier;

  static AppState of(BuildContext context) => maybeOf(context)!;
}

class HarborOfficeApp extends StatefulWidget {
  const HarborOfficeApp({super.key, this.service, this.preferences});

  /// Injectable for tests; when null a real FFI service is created over
  /// the persistent application-support data root.
  final HarborService? service;

  /// Injectable preferences store; when null and [service] is injected an
  /// in-memory store is used, otherwise a file store under app support.
  final PreferencesStore? preferences;

  @override
  State<HarborOfficeApp> createState() => _HarborOfficeAppState();
}

class _HarborOfficeAppState extends State<HarborOfficeApp>
    with WidgetsBindingObserver {
  late final AppState _state;
  HarborService? _service;
  bool _serviceFailed = false;
  String? _serviceError;
  bool _initializing = false;

  HarborService? get service => widget.service ?? _service;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _state = AppState(store: widget.preferences);
    if (widget.preferences != null) _state.load();
    if (widget.service == null) {
      _initializing = true;
      _bootstrap();
    }
  }

  /// Persistent storage: the workspace data root is the app-support
  /// directory (NOT a per-launch temp dir), so keys, preferences and
  /// diagnostics survive restarts. On Android the device root key comes
  /// from the embedding's AndroidKeyStore-backed channel; Apple platforms
  /// use the core's Keychain adapter.
  Future<void> _bootstrap() async {
    HarborService? opened;
    try {
      final support = await getApplicationSupportDirectory();
      if (widget.preferences == null) {
        final prefsPath =
            '${support.path}${Platform.pathSeparator}harbor-office-prefs.json';
        _state.replaceStore(FilePreferencesStore(prefsPath));
        await _state.load();
      }
      final dataRoot =
          Directory('${support.path}${Platform.pathSeparator}harbor-office-data');
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
    } catch (e, stack) {
      DiagnosticsSink.instance.record(
          level: 'error',
          message: 'core start failed: $e',
          context: 'bootstrap',
          stack: stack.toString());
      if (mounted) {
        setState(() {
          _serviceFailed = true;
          _serviceError = e.toString();
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
    DiagnosticsSink.instance.attach(opened);
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    // The view controller is going away for good: release the native
    // core (SQLite handles, keys) deterministically.
    if (state == AppLifecycleState.detached) {
      final s = _service;
      _service = null;
      DiagnosticsSink.instance.attach(null);
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
        final arabic = _state.locale.languageCode == 'ar';
        return MaterialApp(
          onGenerateTitle: (context) => 'Harbor Office Suite',
          debugShowCheckedModeBanner: false,
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
          scrollBehavior: const _OfficeScrollBehavior(),
          // RTL mirrors structure (flutter handles Directionality from
          // locale); technical data keeps intrinsic LTR via Directionality
          // wrappers at usage sites. The effective brightness (including
          // ThemeMode.system) is read back from the resolved Material
          // theme so the Harbor token set always matches it.
          builder: (context, child) {
            final dark = Theme.of(context).brightness == Brightness.dark;
            final colors = dark ? HarborColors.dark : HarborColors.light;
            return AnnotatedRegion<SystemUiOverlayStyle>(
              value: SystemUiOverlayStyle(
                statusBarColor: Colors.transparent,
                statusBarIconBrightness:
                    dark ? Brightness.light : Brightness.dark,
                statusBarBrightness: dark ? Brightness.dark : Brightness.light,
                systemNavigationBarColor: Colors.transparent,
                systemNavigationBarDividerColor: Colors.transparent,
                systemNavigationBarIconBrightness:
                    dark ? Brightness.light : Brightness.dark,
                systemNavigationBarContrastEnforced: false,
              ),
              child: HarborTheme(
                colors: colors,
                text: HarborType(arabic: arabic),
                child: AppStateScope(
                  state: _state,
                  child: HarborServiceProvider(
                    service: service,
                    failed: _serviceFailed,
                    failureDetail: _serviceError,
                    child: child ?? const SizedBox.shrink(),
                  ),
                ),
              ),
            );
          },
          home: _initializing
              ? const _BootingView()
              : OfficeShell(state: _state),
        );
      },
    );
  }
}

/// Mouse-drag scrolling on desktop for lists, filmstrips and the grid.
class _OfficeScrollBehavior extends MaterialScrollBehavior {
  const _OfficeScrollBehavior();

  @override
  Set<PointerDeviceKind> get dragDevices => {
        PointerDeviceKind.touch,
        PointerDeviceKind.mouse,
        PointerDeviceKind.trackpad,
        PointerDeviceKind.stylus,
      };
}

/// Shown while the persistent workspace opens on the worker isolate.
class _BootingView extends StatelessWidget {
  const _BootingView();

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    return Scaffold(
      body: Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const HarborWordmark(size: 24),
            const SizedBox(height: HarborSpace.s2),
            Text(l10n.appStarting, style: t.text.smallOf(t.colors.inkMuted)),
            const SizedBox(height: HarborSpace.s8),
            const SizedBox(
              width: 28,
              height: 28,
              child: CircularProgressIndicator(strokeWidth: 3),
            ),
          ],
        ),
      ),
    );
  }
}

/// Destination metadata for the two office surfaces.
List<HarborDestination> officeDestinations(AppLocalizations l10n) => [
      HarborDestination(l10n.surfaceWork, Icons.description_outlined,
          selectedIcon: Icons.description),
      HarborDestination(l10n.surfaceSettings, Icons.settings_outlined,
          selectedIcon: Icons.settings),
    ];

Widget officeSurfaceFor(int index, AppState state) {
  switch (OfficeSurface.values[index]) {
    case OfficeSurface.work:
      return const WorkSurface();
    case OfficeSurface.settings:
      return SettingsSurface(state: state);
  }
}

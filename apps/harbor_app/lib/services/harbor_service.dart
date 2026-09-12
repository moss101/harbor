import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';

import 'package:harbor_native/harbor_ffi.dart' as ffi;

/// Bridge between the Flutter layer and Harbor Core. Policy and runtime
/// truth stay in Rust; this only surfaces facts and typed errors.
class HarborService extends ChangeNotifier {
  HarborService._(this._client);

  final ffi.HarborCoreClient _client;

  factory HarborService.open({
    required String libraryPath,
    required String dataRoot,
    String workspaceId = 'ws-default',
  }) {
    final client = ffi.HarborCoreClient.open(
        libraryPath, dataRoot, workspaceId, ffi.HarborPrivacyMode.localOnly);
    return HarborService._(client);
  }

  String _policy = '…';
  String _execution = '…';
  List<Map<String, dynamic>> _installedModels = [];
  List<Map<String, dynamic>> _runs = [];
  Map<String, dynamic>? _preview;

  String get policy => _policy;
  String get execution => _execution;
  List<Map<String, dynamic>> get installedModels => _installedModels;
  List<Map<String, dynamic>> get runs => _runs;
  Map<String, dynamic>? get preview => _preview;

  /// Load Trust Pulse facts, installed models and durable runs from core.
  Future<void> refresh() async {
    final pulse = _client.trustPulse();
    _policy = 'Policy: ${pulse['policy']}';
    _execution = 'Execution: ${pulse['execution_hint']}';
    try {
      _installedModels = _client.installedModels();
    } on ffi.HarborCoreException {
      _installedModels = [];
    }
    try {
      _runs = _client.listRuns();
    } on ffi.HarborCoreException {
      _runs = [];
    }
    notifyListeners();
  }

  Map<String, dynamic>? createRun(String runId) {
    try {
      return _client.createRun(runId);
    } on ffi.HarborCoreException {
      return null;
    }
  }

  Map<String, dynamic>? replayRun(String runId) {
    try {
      return _client.replayRun(runId);
    } on ffi.HarborCoreException {
      return null;
    }
  }

  Map<String, dynamic>? fitScore(String packageId) {
    try {
      return _client.fitScore(
        packageId: packageId,
        gpuBackend: true,
        accelerated: true,
      );
    } on ffi.HarborCoreException {
      return null;
    }
  }

  /// Load a Work Canvas preview for artifact bytes through the core.
  Future<void> loadPreviewFromBytes(List<int> bytes) async {
    try {
      _preview = _client.previewArtifact(bytes);
    } on ffi.HarborCoreException {
      _preview = null;
    }
    notifyListeners();
  }

  void close() => _client.close();
}

/// Access + default library location resolution.
class HarborBinding {
  static String defaultLibraryPath() {
    const fromEnv = String.fromEnvironment('HARBOR_FFI_LIB');
    if (fromEnv.isNotEmpty) return fromEnv;
    return 'libharbor_ffi.dylib';
  }
}

/// Inherited access to the live core service. `failed` reports that the
/// native library could not be loaded — surfaces then render honest
/// degraded states rather than fake data.
class HarborServiceProvider extends InheritedNotifier<HarborService> {
  const HarborServiceProvider({
    super.key,
    required HarborService? service,
    required this.failed,
    required super.child,
  }) : super(notifier: service);

  final bool failed;

  static HarborServiceProvider of(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<HarborServiceProvider>()!;
}

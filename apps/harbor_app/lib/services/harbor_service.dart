import 'dart:convert' show base64Encode;
import 'dart:io' show Platform;
import 'dart:math';

import 'package:flutter/widgets.dart';

import 'package:harbor_domain/harbor_domain.dart';
import 'package:harbor_native/harbor_ffi.dart' as ffi;
import 'package:harbor_native/harbor_worker.dart';

/// Bridge between the Flutter layer and Harbor Core. Policy and runtime
/// truth stay in Rust; this only surfaces facts and typed errors.
///
/// Every core call runs on a dedicated background isolate (see
/// [HarborCoreWorker]): the UI isolate never blocks on FFI. Long-running
/// work (model acquisition, knowledge ingestion, grounded generation)
/// executes on native threads in the core and is surfaced here through
/// [opProgress] snapshots the UI can render and cancel.
class HarborService extends ChangeNotifier {
  HarborService._(this._worker);

  final HarborCoreWorker _worker;

  /// Opens the native core on the worker isolate. [deviceRootHex] injects
  /// an embedding-layer device root (Android Keystore unseal path); when
  /// null the core selects the platform keystore (Keychain / DPAPI).
  static Future<HarborService> open({
    required String libraryPath,
    required String dataRoot,
    String workspaceId = 'ws-default',
    String? deviceRootHex,
  }) async {
    final worker = await HarborCoreWorker.spawn(
      libraryPath: libraryPath,
      dataRoot: dataRoot,
      workspaceId: workspaceId,
      privacyMode: ffi.HarborPrivacyMode.localOnly.index,
      deviceRootHex: deviceRootHex,
    );
    return HarborService._(worker);
  }

  String _policy = '…';
  String _execution = '…';
  String? _policyCode;
  String? _executionHint;
  String? _policyVersion;
  String? _workspaceId;
  List<Map<String, dynamic>> _installedModels = [];
  List<Map<String, dynamic>> _runs = [];
  List<SkillSummary> _skills = [];
  Map<String, dynamic>? _preview;
  String? _previewName;
  String? _previewError;
  bool _previewLoading = false;
  String? _deviceId;

  /// Display strings ("Policy: LOCAL_ONLY") kept for the Lens/Settings.
  String get policy => _policy;
  String get execution => _execution;

  /// Raw core facts (policy token, execution hint, policy version).
  String? get policyCode => _policyCode;
  String? get executionHint => _executionHint;
  String? get policyVersion => _policyVersion;
  String? get workspaceId => _workspaceId;
  List<Map<String, dynamic>> get installedModels => _installedModels;
  List<Map<String, dynamic>> get runs => _runs;
  List<SkillSummary> get skills => _skills;
  Map<String, dynamic>? get preview => _preview;

  /// File name of the open Work Canvas artifact (presentation only).
  String? get previewName => _previewName;

  /// Core error for the last failed preview load (null when none).
  String? get previewError => _previewError;
  bool get previewLoading => _previewLoading;
  String? get deviceId => _deviceId;

  /// Whether the last refresh reached the core at all.
  bool get coreReachable => _policyCode != null;

  bool knowledgeOpen = false;
  int knowledgeDimension = 0;
  String? knowledgeIdentity;
  List<Map<String, dynamic>> knowledgeSources = [];

  /// Live snapshots of running background ops, keyed by op id. The UI
  /// renders these (progress bars, cancel buttons) while work executes.
  final Map<String, Map<String, dynamic>> opProgress = {};

  /// Latest progress snapshot per op kind ('acquire' | 'generate' |
  /// 'ingest') for UI that tracks work by kind rather than op id.
  final Map<String, Map<String, dynamic>> kindProgress = {};

  Future<Map<String, dynamic>> _call(String method,
      [Map<String, dynamic>? args]) async {
    final result = await _worker.request(method, args);
    return (result as Map).cast<String, dynamic>();
  }

  List<Map<String, dynamic>> _mapList(dynamic value) {
    if (value is! List) return const [];
    return value.cast<Map>().map((m) => m.cast<String, dynamic>()).toList();
  }

  /// Load Trust Pulse facts, installed models, durable runs, skills and
  /// knowledge sources from core. All through the worker isolate.
  Future<void> refresh() async {
    try {
      final pulse = await _call('trust.pulse');
      _policyCode = pulse['policy'] as String?;
      _executionHint = pulse['execution_hint'] as String?;
      _policyVersion = pulse['policy_version']?.toString();
      _policy = 'Policy: ${pulse['policy']}';
      _execution = 'Execution: ${pulse['execution_hint']}';
    } on ffi.HarborCoreException {
      // Keep the placeholder-free last-known facts.
    }
    try {
      final identity = await _call('identity.get');
      _deviceId = identity['device_id'] as String?;
      _workspaceId = identity['workspace_id'] as String?;
    } on ffi.HarborCoreException {
      _deviceId = null;
    }
    try {
      _installedModels = _mapList((await _call('models.installed'))['models']);
    } on ffi.HarborCoreException {
      _installedModels = [];
    }
    try {
      _runs = _mapList((await _call('runs.list'))['runs']);
    } on ffi.HarborCoreException {
      _runs = [];
    }
    try {
      final result = await _call('skills.list');
      _skills = _mapList(result['skills']).map(SkillSummary.fromMap).toList();
    } on ffi.HarborCoreException {
      _skills = [];
    }
    if (knowledgeOpen) {
      try {
        final result = await _call('knowledge.sources');
        knowledgeSources = _mapList(result['sources']);
        knowledgeIdentity = result['identity'] as String?;
      } on ffi.HarborCoreException {
        // Index went away (e.g. model uninstalled): report honestly.
        knowledgeOpen = false;
        knowledgeSources = [];
      }
    }
    notifyListeners();
  }

  /// Cryptographically random run id — timestamps are NOT ids (colliding
  /// ids made the durable log reject every submission after the first).
  static String newRunId() {
    final rng = Random.secure();
    final bytes = List<int>.generate(12, (_) => rng.nextInt(256));
    return 'run-${bytes.map((b) => b.toRadixString(16).padLeft(2, '0')).join()}';
  }

  Future<Map<String, dynamic>?> createRun(String runId) async {
    try {
      return await _call('run.create', {'run_id': runId});
    } on ffi.HarborCoreException {
      return null;
    }
  }

  /// Full Home-composer flow: create the durable run and log the user's
  /// request as its first step. Returns the run id or null on failure.
  Future<String?> submitRequest(String text) async {
    final runId = newRunId();
    try {
      await _call('run.create', {'run_id': runId});
      await _call('run.log_request', {'run_id': runId, 'text': text});
    } on ffi.HarborCoreException {
      return null;
    }
    await refresh();
    return runId;
  }

  Future<Map<String, dynamic>?> replayRun(String runId) async {
    try {
      return await _call('run.replay', {'run_id': runId});
    } on ffi.HarborCoreException {
      return null;
    }
  }

  /// Durable counters for one run (steps, tools, context tokens, compute).
  Future<Map<String, dynamic>?> runState(String runId) async {
    try {
      return await _call('run.state', {'run_id': runId});
    } on ffi.HarborCoreException {
      return null;
    }
  }

  /// Every background op the core still remembers for this process
  /// (running and finished). Order is not guaranteed; the UI sorts.
  Future<List<Map<String, dynamic>>> listOps() async {
    try {
      return _mapList((await _call('op.list'))['ops']);
    } on ffi.HarborCoreException {
      return const [];
    }
  }

  /// Live kind-tracked progress snapshots (acquire / ingest / generate).
  List<Map<String, dynamic>> get activeOps =>
      kindProgress.values.toList(growable: false);

  /// Search public HF repositories (acquisition metadata only). Returns
  /// null when the search itself failed (network/broker), so the UI can
  /// tell failure apart from "no matches".
  Future<List<Map<String, dynamic>>?> searchHuggingFace(String query) async {
    try {
      final r = await _call('models.search_hf', {'query': query, 'limit': 8});
      return _mapList(r['models']);
    } on ffi.HarborCoreException {
      return null;
    }
  }

  /// A repo's GGUF weight files (brokered metadata read) — the real file
  /// set an install launches with.
  Future<List<Map<String, dynamic>>> huggingFaceFiles(String repoId) async {
    try {
      final r = await _call('models.hf_files', {'repo_id': repoId});
      return _mapList(r['files']);
    } on ffi.HarborCoreException {
      return [];
    }
  }

  /// Acquire a model package as a cancellable background op: brokered
  /// download + staged install + hash identity, with live progress in
  /// [kindProgress]['acquire']. Throws [ffi.HarborCoreException] on
  /// failure or cancellation (honest, never silently null).
  Future<Map<String, dynamic>> acquireModelHf({
    required String packageId,
    required String repoId,
    required List<Map<String, String>> files,
  }) async {
    final result = await _runOp('op.start_acquire', {
      'package_id': packageId,
      'repo_id': repoId,
      'files': files,
    });
    await refresh();
    return result;
  }

  /// Install a model from a local file the core reads itself (no base64
  /// round trip through the boundary).
  Future<bool> installModelFromPath({
    required String packageId,
    required String path,
    String role = 'weights',
  }) async {
    try {
      await _call('models.install_from_path', {
        'package_id': packageId,
        'path': path,
        'role': role,
      });
      await refresh();
      return true;
    } on ffi.HarborCoreException {
      return false;
    }
  }

  /// Open the durable knowledge index over an installed embedding model.
  /// Reports failure honestly (no embedding model installed) instead of
  /// fabricating search results.
  Future<bool> openKnowledge({String packageId = 'bge-small-en-v1.5'}) async {
    try {
      final r = await _call('knowledge.open', {'package_id': packageId});
      knowledgeOpen = true;
      knowledgeDimension = r['dimension'] as int;
      knowledgeIdentity = r['identity'] as String?;
      await refresh();
      return true;
    } on ffi.HarborCoreException {
      knowledgeOpen = false;
      notifyListeners();
      return false;
    }
  }

  /// Ingest sources as a cancellable background op with chunk-level
  /// progress. Ingesting an existing source id replaces its chunks.
  Future<Map<String, dynamic>?> ingestSources(
      List<Map<String, dynamic>> sources) async {
    Map<String, dynamic>? result;
    try {
      result = await _runOp('op.start_ingest', {'sources': sources});
    } on ffi.HarborCoreException {
      result = null;
    }
    await refresh();
    return result;
  }

  Future<bool> removeKnowledgeSource(String sourceId) async {
    try {
      await _call('knowledge.remove_source', {'source_id': sourceId});
      await refresh();
      return true;
    } on ffi.HarborCoreException {
      return false;
    }
  }

  Future<Map<String, dynamic>?> searchKnowledge(String question,
      {int topK = 5}) async {
    if (!knowledgeOpen) return null;
    try {
      return await _call(
          'knowledge.search', {'question': question, 'top_k': topK});
    } on ffi.HarborCoreException {
      return null;
    }
  }

  /// Grounded generation as a cancellable background op. When [runId] is
  /// given the question and answer become durable run events (Activity).
  /// Throws [ffi.HarborCoreException] on failure or cancellation.
  Future<Map<String, dynamic>> generateAnswer(String question,
      {required String chatPackage, int maxTokens = 256, String? runId}) async {
    final answer = await _runOp('op.start_generate', {
      'question': question,
      'chat_package': chatPackage,
      'max_tokens': maxTokens,
      if (runId != null) 'run_id': runId,
    });
    if (runId != null) await refresh();
    return answer;
  }

  /// Run a graph skill as a cancellable background op (decision 0006).
  /// Artifacts are passed as bytes: this layer holds the user-granted file
  /// handles, the core never sees a path. Returns the run report; a
  /// report in `WAITING_APPROVAL` carries `status.approval` for
  /// [decideRun]. Throws [ffi.HarborCoreException] on failure.
  Future<Map<String, dynamic>> startSkillRun({
    required String skillId,
    required Map<String, dynamic> inputs,
    List<SkillArtifact> artifacts = const [],
    Map<String, dynamic> hostInputs = const {},
    String? chatPackage,
  }) async {
    final report = await _runOp('op.start_skill_run', {
      'skill_id': skillId,
      'inputs': inputs,
      'host_inputs': hostInputs,
      if (chatPackage != null) 'chat_package': chatPackage,
      'artifacts': [
        for (final a in artifacts)
          {'id': a.id, 'name': a.name, 'data_b64': base64Encode(a.bytes)},
      ],
    });
    await refresh();
    return report;
  }

  /// Decide a pending approval; the run continues from the approval node
  /// and the returned report is the run's new terminal picture.
  Future<Map<String, dynamic>> decideRun(String runId,
      {required bool approved}) async {
    final report =
        await _call('run.decide', {'run_id': runId, 'approved': approved});
    await refresh();
    return report;
  }

  /// Durable picture of a graph run (trail with node ids and io hashes,
  /// outcome, pending approval), read from the encrypted snapshot store.
  Future<Map<String, dynamic>?> runSnapshot(String runId) async {
    try {
      return await _call('run.snapshot', {'run_id': runId});
    } on ffi.HarborCoreException {
      return null;
    }
  }

  /// Cancel a running background op (acquisition, ingest, generation).
  Future<void> cancelOp(String opId) async {
    try {
      await _call('op.cancel', {'op_id': opId});
    } on ffi.HarborCoreException {
      // The op may have finished between the render and the cancel.
    }
  }

  /// One background op end-to-end: start it, poll real progress into
  /// [opProgress] (UI re-renders on each change), resolve with the result.
  Future<Map<String, dynamic>> _runOp(
      String startMethod, Map<String, dynamic> args) async {
    final started = await _call(startMethod, args);
    final opId = started['op_id'] as String;
    while (true) {
      final status = await _call('op.status', {'op_id': opId});
      final state = status['state'] as String;
      final changed = _updateProgress(opId, status);
      if (changed) notifyListeners();
      if (state == 'done') {
        opProgress.remove(opId);
        kindProgress.remove(status['kind']);
        notifyListeners();
        return (status['result'] as Map).cast<String, dynamic>();
      }
      if (state == 'failed' || state == 'cancelled') {
        final error = (status['result']?['error'] ?? state).toString();
        opProgress.remove(opId);
        kindProgress.remove(status['kind']);
        notifyListeners();
        throw ffi.HarborCoreException(error);
      }
      await Future<void>.delayed(const Duration(milliseconds: 250));
    }
  }

  bool _updateProgress(String opId, Map<String, dynamic> status) {
    final previous = opProgress[opId];
    if (previous != null &&
        previous['phase'] == status['phase'] &&
        previous['detail'] == status['detail'] &&
        previous['bytes_done'] == status['bytes_done'] &&
        previous['items_done'] == status['items_done'] &&
        previous['bytes_total'] == status['bytes_total'] &&
        previous['items_total'] == status['items_total']) {
      return false;
    }
    opProgress[opId] = Map<String, dynamic>.of(status);
    if (status['kind'] is String) {
      kindProgress[status['kind'] as String] = Map<String, dynamic>.of(status);
    }
    return true;
  }

  /// Clear a finished kind-tracked progress entry (called by surfaces
  /// after they render the terminal state).
  void clearKindProgress(String kind) {
    if (kindProgress.remove(kind) != null) notifyListeners();
  }

  Future<Map<String, dynamic>?> fitScore(String packageId) async {
    try {
      return await _call('model.fit_score', {
        'package_id': packageId,
        'gpu_backend': true,
        'accelerated': true,
      });
    } on ffi.HarborCoreException {
      return null;
    }
  }

  /// Load a Work Canvas preview for artifact bytes through the core.
  /// Failure is reported (not swallowed) so the canvas can explain it.
  Future<void> loadPreviewFromBytes(List<int> bytes, {String? name}) async {
    _previewLoading = true;
    _previewError = null;
    notifyListeners();
    try {
      _preview =
          await _call('artifact.preview', {'data_b64': base64Encode(bytes)});
      _previewName = name;
    } on ffi.HarborCoreException catch (e) {
      _preview = null;
      _previewName = name;
      _previewError = e.message;
    }
    _previewLoading = false;
    notifyListeners();
  }

  /// Close the open artifact (presentation state only).
  void clearPreview() {
    _preview = null;
    _previewName = null;
    _previewError = null;
    notifyListeners();
  }

  /// Preview IR WITHOUT touching the Work Canvas state (used by knowledge
  /// ingestion to extract docx/pdf text through the qualified paths).
  Future<Map<String, dynamic>?> extractPreview(List<int> bytes) async {
    try {
      return await _call('artifact.preview', {'data_b64': base64Encode(bytes)});
    } on ffi.HarborCoreException {
      return null;
    }
  }

  /// Release the native core (idempotent). The worker closes the handle
  /// inside its isolate, then stops.
  Future<void> close() async {
    await _worker.close();
  }

  @override
  void dispose() {
    _worker.close();
    super.dispose();
  }
}

/// Access + default library location resolution.
class HarborBinding {
  static const String fromEnv = String.fromEnvironment('HARBOR_FFI_LIB');

  static String defaultLibraryPath() {
    if (fromEnv.isNotEmpty) return fromEnv;
    if (Platform.isAndroid) return 'libharbor_ffi.so';
    if (Platform.isWindows) return 'harbor_ffi.dll';
    if (Platform.isIOS) {
      // Device builds force-load the libharbor_ffi.a static archive into
      // the main executable (Stage Harbor Native Archive build phase), so
      // the client falls back to process-symbol lookup. Simulator builds
      // embed the dylib at the app-bundle root; @executable_path resolves
      // against Runner.app/.
      return '@executable_path/libharbor_ffi.dylib';
    }
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
    this.failureDetail,
  }) : super(notifier: service);

  final bool failed;

  /// Technical reason the core could not be opened (shown verbatim in the
  /// degraded state so the cause is never hidden).
  final String? failureDetail;

  static HarborServiceProvider of(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<HarborServiceProvider>()!;
}

/// Bytes of a file the user attached to a skill run, keyed by the id the
/// run inputs reference (e.g. `artifact_id: "a1"`).
final class SkillArtifact {
  const SkillArtifact(
      {required this.id, required this.name, required this.bytes});
  final String id;
  final String name;
  final List<int> bytes;
}

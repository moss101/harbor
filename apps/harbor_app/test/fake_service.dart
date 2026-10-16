import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/l10n/app_localizations.dart';
import 'package:harbor_app/main.dart';
import 'package:harbor_app/services/harbor_service.dart';
import 'package:harbor_domain/harbor_domain.dart';
import 'package:harbor_native/harbor_ffi.dart' as ffi;
import 'package:harbor_ui/harbor_ui.dart';

/// A [HarborService] with no core behind it: each call a journey needs is
/// scripted here and recorded, so routing and the composers are tested
/// deterministically, without the dylib. Anything not overridden fails
/// with a typed core error, exactly like a refusing core.
class FakeHarborService extends HarborService {
  FakeHarborService({
    this.models = const [],
    this.skillList = const [],
    this.intent,
    this.suggestion,
    this.indexOpens = true,
  }) : super.detached();

  List<Map<String, dynamic>> models;
  List<SkillSummary> skillList;

  /// What `decision.choose` / `skills.suggest` answer (null: no index).
  Map<String, dynamic>? intent;
  Map<String, dynamic>? suggestion;

  /// Whether [openKnowledge] succeeds.
  bool indexOpens;

  Map<String, dynamic>? fakePreview;
  String? fakePreviewName;
  List<int>? fakeBytes;
  String? fakePath;

  final List<String> calls = [];
  final List<Map<String, dynamic>> askAboutFileArgs = [];
  final List<Map<String, dynamic>> generateArgs = [];

  /// What the generation ops return.
  Map<String, dynamic> answer = const {
    'answer': 'Revenue grew 12% [p1].',
    'used_citations': true,
    'execution': 'ON_DEVICE',
    'executed_on': 'cpu',
    'citations': [],
  };

  @override
  List<Map<String, dynamic>> get installedModels => models;
  @override
  bool get needsFirstModel => models.isEmpty;
  @override
  List<SkillSummary> get skills => skillList;
  @override
  List<Map<String, dynamic>> get runs => const [];
  @override
  Map<String, dynamic>? get preview => fakePreview;
  @override
  String? get previewName => fakePreviewName;
  @override
  List<int>? get previewSourceBytes => fakeBytes;
  @override
  String? get previewPath => fakePath;
  @override
  bool get previewLoading => false;
  @override
  String? get previewError => null;

  @override
  Future<void> refresh() async {}

  /// Tell listeners the scripted state changed.
  void notify() => notifyListeners();

  @override
  Future<bool> openKnowledge({String packageId = 'bge-small-en-v1.5'}) async {
    calls.add('openKnowledge');
    knowledgeOpen = indexOpens;
    return indexOpens;
  }

  @override
  Future<Map<String, dynamic>?> suggestIntent(String text) async {
    calls.add('suggestIntent');
    return knowledgeOpen ? intent : null;
  }

  @override
  Future<Map<String, dynamic>?> suggestSkills(String text,
      {String? chatPackage}) async {
    calls.add('suggestSkills');
    return knowledgeOpen ? suggestion : null;
  }

  @override
  Future<Map<String, dynamic>?> createRun(String runId) async {
    calls.add('createRun');
    return {'run_id': runId};
  }

  /// When set, generation runs as a live `generate` op until this
  /// completes (or [cancelOp] cancels it), so cancel can be exercised.
  Completer<void>? generateGate;

  /// Run ids generation was bound to (durable runs).
  final List<String?> generateRunIds = [];

  @override
  Future<Map<String, dynamic>> generateAnswer(String question,
      {required String chatPackage, int maxTokens = 256, String? runId}) async {
    calls.add('generateAnswer');
    generateArgs.add({'question': question, 'chat_package': chatPackage});
    generateRunIds.add(runId);
    final gate = generateGate;
    if (gate != null) {
      kindProgress['generate'] = {
        'op_id': 'op-generate',
        'kind': 'generate',
        'items_done': 3,
      };
      notifyListeners();
      try {
        await gate.future;
      } finally {
        kindProgress.remove('generate');
        notifyListeners();
      }
    }
    return answer;
  }

  @override
  Future<void> cancelOp(String opId) async {
    calls.add('cancelOp:$opId');
    final gate = generateGate;
    if (gate != null && !gate.isCompleted) {
      gate.completeError(ffi.HarborCoreException('op cancelled'));
    }
  }

  /// What retrieval-only search returns.
  Map<String, dynamic>? evidence = const {'citations': []};

  @override
  Future<Map<String, dynamic>?> searchKnowledge(String question,
      {int topK = 5}) async {
    calls.add('searchKnowledge');
    return knowledgeOpen ? evidence : null;
  }

  @override
  Future<Map<String, dynamic>> uninstallPreview(String packageId) async {
    calls.add('uninstallPreview:$packageId');
    return {
      'package_id': packageId,
      'scope_digest': 'digest',
      'manifest_bytes_total': 1 << 30,
      'files': ['weights.gguf'],
      'in_use': false,
    };
  }

  @override
  Future<Map<String, dynamic>> askAboutFile(String question,
      {required String chatPackage,
      String? runId,
      int maxTokens = 384,
      FileDocument? document}) async {
    calls.add('askAboutFile');
    final doc = document ?? FileDocument.fromPreview(previewName, preview)!;
    askAboutFileArgs.add({
      'question': question,
      'chat_package': chatPackage,
      'run_id': runId,
      'document': doc.toWire(),
    });
    return answer;
  }

  @override
  Future<Map<String, dynamic>> startSkillRun({
    required String skillId,
    required Map<String, dynamic> inputs,
    List<SkillArtifact> artifacts = const [],
    Map<String, dynamic> hostInputs = const {},
    String? chatPackage,
    String? runId,
  }) async {
    calls.add('startSkillRun');
    return startReport;
  }

  /// What a started skill run reports.
  Map<String, dynamic> startReport = const {
    'run_id': 'run-fake',
    'state': 'COMPLETED',
    'status': {},
  };

  /// What committing the proposal reports (`commit.destination` is where
  /// the approved file was written).
  Map<String, dynamic>? commitResult;

  @override
  Future<Map<String, dynamic>> commitProposal({
    required String runId,
    required String destination,
    List<SkillArtifact> artifacts = const [],
    CommitTarget target = CommitTarget.saveNewCopy,
  }) async {
    calls.add('commitProposal');
    return commitResult ??
        {
          'report': {'run_id': runId, 'state': 'COMPLETED', 'status': {}},
          'commit': {
            'destination': destination,
            'mode': 'new_copy',
            'version_id': 'v1'
          },
        };
  }

  @override
  Future<Map<String, dynamic>?> extractPreview(List<int> bytes) async {
    calls.add('extractPreview');
    return fakePreview;
  }

  @override
  Future<void> loadPreviewFromBytes(List<int> bytes,
      {String? name, String? path}) async {
    calls.add('loadPreviewFromBytes');
    fakeBytes = bytes;
    fakePreviewName = name;
    fakePath = path;
    notifyListeners();
  }

  @override
  void clearPreview() {
    fakePreview = null;
    fakePreviewName = null;
    fakeBytes = null;
    notifyListeners();
  }
}

/// A runnable graph skill whose inputs are [inputs] (JSON-schema props).
SkillSummary graphSkill(String id, String title,
    {Map<String, dynamic> inputs = const {},
    List<String> required = const [],
    String description = ''}) {
  return SkillSummary(
    id: id,
    title: title,
    family: 'documents',
    description: description,
    tools: const [],
    runnable: true,
    graph: SkillGraphInfo(
      id: id,
      version: 1,
      nodeCount: 3,
      modelNodes: 1,
      tools: const [],
      inputs: {'properties': inputs, 'required': required},
      maxSteps: 10,
      maxToolCalls: 5,
    ),
  );
}

/// A file-taking skill (an artifact plus an instruction).
final tidySkill = graphSkill('table-cleanup', 'Table Cleanup',
    inputs: {
      'artifact_id': {'type': 'string'},
      'instruction': {'type': 'string', 'maxLength': 600},
    },
    required: ['artifact_id'],
    description: 'Clean up a table in a workbook.');

/// A text-only skill.
final emailSkill = graphSkill('email-drafting', 'Email Drafting',
    inputs: {
      'notes': {'type': 'string', 'maxLength': 6000},
    },
    required: ['notes'],
    description: 'Draft an email from notes.');

/// A docx preview with three paragraphs (one empty).
Map<String, dynamic> docxPreview() => {
      'kind': 'docx',
      'preview': {
        'paragraphs': [
          {'index': 0, 'text': 'Quarterly report', 'style': 'Title'},
          {'index': 1, 'text': ''},
          {'index': 2, 'text': 'Revenue grew 12% over the quarter.'},
        ],
      },
    };

/// Pump [child] under the app's localizations, theme, AppState and the
/// [service], at a real viewport of [size].
Future<AppState> pumpSurface(
  WidgetTester tester,
  HarborService service,
  Widget Function(AppState state) child, {
  Size size = const Size(1280, 900),
  Locale locale = const Locale('en'),
}) async {
  tester.view.physicalSize = size;
  tester.view.devicePixelRatio = 1.0;
  addTearDown(tester.view.reset);
  final state = AppState();
  final arabic = locale.languageCode == 'ar';
  await tester.pumpWidget(MaterialApp(
    locale: locale,
    localizationsDelegates: const [
      AppLocalizations.delegate,
      GlobalMaterialLocalizations.delegate,
      GlobalWidgetsLocalizations.delegate,
      GlobalCupertinoLocalizations.delegate,
    ],
    supportedLocales: const [Locale('en'), Locale('ar')],
    theme: harborThemeData(dark: false, arabic: arabic),
    builder: (_, inner) => HarborTheme(
      colors: HarborColors.light,
      text: HarborType(arabic: arabic),
      child: inner!,
    ),
    home: AppStateScope(
      state: state,
      child: HarborServiceProvider(
        service: service,
        failed: false,
        child: Scaffold(body: child(state)),
      ),
    ),
  ));
  await tester.pump();
  return state;
}

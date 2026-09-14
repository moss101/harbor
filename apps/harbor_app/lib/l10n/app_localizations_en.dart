// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for English (`en`).
class AppLocalizationsEn extends AppLocalizations {
  AppLocalizationsEn([String locale = 'en']) : super(locale);

  @override
  String get surfaceHome => 'Home';

  @override
  String get surfaceAsk => 'Ask';

  @override
  String get surfaceWork => 'Work';

  @override
  String get surfaceAgents => 'Agents';

  @override
  String get surfaceModels => 'Models';

  @override
  String get surfaceSkills => 'Skills';

  @override
  String get surfaceKnowledge => 'Knowledge';

  @override
  String get surfaceActivity => 'Activity';

  @override
  String get surfaceSettings => 'Settings';

  @override
  String get homeHeadline => 'What do you want to get done?';

  @override
  String get homeComposerHint => 'Describe the work, attach files, and go.';

  @override
  String get quickSummarize => 'Summarize document';

  @override
  String get quickAnalyze => 'Analyze spreadsheet';

  @override
  String get quickPresent => 'Create presentation';

  @override
  String get quickCompare => 'Compare files';

  @override
  String get quickOrganize => 'Organize project';

  @override
  String get quickResearch => 'Research this folder';

  @override
  String get quickTranslate => 'Translate content';

  @override
  String get workEmptyTitle => 'No artifact open';

  @override
  String get workEmptyBody =>
      'Open a document, workbook, deck or PDF to see it here with version, verification and conflict state.';

  @override
  String get modelsInstalled => 'Installed';

  @override
  String get modelsRecommended => 'Recommended';

  @override
  String get modelsLibrary => 'Harbor Library';

  @override
  String get modelsHuggingFace => 'Hugging Face';

  @override
  String get modelsImport => 'Import';

  @override
  String get modelsBenchmark => 'Benchmark';

  @override
  String get fitScore => 'Fit Score';

  @override
  String get trustPolicy => 'Policy: LOCAL ONLY';

  @override
  String get trustExecutionOnDevice => 'Execution: ON DEVICE';

  @override
  String get runTrailEmpty => 'No activity yet.';

  @override
  String get approvalApprove => 'Approve';

  @override
  String get approvalDeny => 'Deny';

  @override
  String get settingsLanguage => 'Language';

  @override
  String get settingsEnglish => 'English';

  @override
  String get settingsArabic => 'العربية';

  @override
  String get settingsTheme => 'Theme';

  @override
  String get settingsThemeLight => 'Light';

  @override
  String get settingsThemeDark => 'Dark';

  @override
  String get settingsPrivacy => 'Workspace privacy';

  @override
  String get agentsEmptyTitle => 'No agents configured';

  @override
  String get agentsEmptyBody =>
      'Agents combine a model, tools, skills, knowledge and policy. Create one to delegate multi-step work with durable, inspectable runs.';

  @override
  String get skillsEmptyTitle => 'Built-in skills ready';

  @override
  String get skillsEmptyBody =>
      'Ship-quality skills for documents, spreadsheets, research, translation and more.';

  @override
  String get knowledgeEmptyTitle => 'Knowledge is empty';

  @override
  String get knowledgeEmptyBody =>
      'Add folders or documents to build a local, citation-backed index. Sources never leave the device under Local Only.';

  @override
  String get activityEmptyTitle => 'No runs yet';

  @override
  String get activityEmptyBody =>
      'Durable agent runs appear here with their full Run Trail — including pause, approval and recovery states.';

  @override
  String get askEmptyTitle => 'Ask works on your files';

  @override
  String get askEmptyBody =>
      'Answers ground in your workspace with citations and abstain when evidence is insufficient.';

  @override
  String get lensButton => 'Lens';

  @override
  String workCanvasRuleActive(int min) {
    return 'canvas ≥ ${min}px rule active';
  }

  @override
  String get canvasViewportEditing => 'viewport-sized editing';

  @override
  String get openFile => 'Open file';

  @override
  String sheetLabel(String name) {
    return 'Sheet: $name';
  }

  @override
  String get attachFilesTooltip => 'Attach files';

  @override
  String get modelDockEmpty =>
      'No model installed — open Models to install one';

  @override
  String get modelDockCoreUnavailable =>
      'Core unavailable — native runtime not loaded';

  @override
  String get modelsLibraryEmpty =>
      'Curated Harbor library packages appear here.';

  @override
  String get modelsHfEmpty =>
      'Search public repositories. Model packages are data — no repository code ever executes.';

  @override
  String get coreNotLoadedModels =>
      'Native core not loaded; installed models are unavailable. Build core/harbor_ffi to enable this view.';

  @override
  String get modelsInstalledEmpty =>
      'Install a model from Recommended or the Library. Fit Score shows what your device can run well.';

  @override
  String get modelsBenchmarkEmpty =>
      'Controlled device-local benchmark workloads with model/runtime/device identity.';

  @override
  String get modelsRecommendedEmpty =>
      'Recommendations appear once the catalog is synced. A model is recommended only when your device can run it well.';

  @override
  String get coreNotLoadedSkills =>
      'Native core not loaded; skills are declared in the core and cannot be listed.';

  @override
  String get coreNotLoadedActivity =>
      'Native core not loaded; durable runs live in the core store and cannot be listed.';

  @override
  String get newAgent => 'New agent';

  @override
  String get addSources => 'Add sources';

  @override
  String toolsCount(int count) {
    return '$count tools';
  }

  @override
  String filesSizeRuntime(int files, int mb, String runtime) {
    return '$files files · $mb MB · runtime $runtime';
  }

  @override
  String runStateLine(String state, int ms) {
    return 'state: $state · $ms ms executor time';
  }

  @override
  String scoreLine(String pct, String state) {
    return 'score $pct% · $state';
  }

  @override
  String get askNoEvidenceTitle => 'No supporting evidence';

  @override
  String get askNoEvidenceBody =>
      'Nothing in the local index supports this question, so I am abstaining rather than guessing.';

  @override
  String get askKnowledgeNotOpen =>
      'Knowledge is not open yet. Install an embedding model (Models → Installed) to ground answers locally.';

  @override
  String get askAbstentionHeading =>
      'I could not find support for this in your Knowledge.';

  @override
  String get sendAction => 'Send';

  @override
  String get lensOpenTooltip => 'Open the Harbor Lens inspector';

  @override
  String get askSearchTooltip => 'Search Knowledge';

  @override
  String get statusInstalled => 'INSTALLED';

  @override
  String get cancelAction => 'Cancel';

  @override
  String get askGenerateTooltip => 'Generate answer';

  @override
  String get askAnswerHeading => 'Answer';

  @override
  String get askCitationsHeading => 'Cited sources';

  @override
  String askExecutedOnLine(String model, int tokens) {
    return 'Executed on $model · $tokens tokens on-device';
  }

  @override
  String get askGenerating => 'Generating on-device…';

  @override
  String askGenerationProgress(int tokens) {
    return '$tokens tokens generated';
  }

  @override
  String get askInsufficientNote =>
      'The model reports the evidence is insufficient — this answer is not grounded in your sources.';

  @override
  String get askNoChatModel =>
      'No chat model installed. Answers need a chat model (Models → Hugging Face); with Knowledge open, retrieval-only citations still work.';

  @override
  String get askSearchOnly => 'Search Knowledge';

  @override
  String get knowledgeSourcesHeading => 'Indexed sources';

  @override
  String knowledgeChunksCount(int count) {
    return '$count chunks';
  }

  @override
  String get knowledgeRemoveAction => 'Remove';

  @override
  String knowledgeIngesting(int done, int total) {
    return 'Indexing chunks $done/$total';
  }

  @override
  String get knowledgeOpenNeedsModel =>
      'Install the embedding model bge-small-en-v1.5 (Models tab) to build the local index.';

  @override
  String get knowledgeAddFilesTooltip => 'Add files to the local index';

  @override
  String get knowledgeAddTextAction => 'Paste text';

  @override
  String get knowledgeAddTextTitle => 'Index pasted text';

  @override
  String get knowledgeAddTextTitleHint => 'Title';

  @override
  String get knowledgeAddTextBodyHint => 'Paste the text to index';

  @override
  String get knowledgeAddTextConfirm => 'Index';

  @override
  String knowledgeSourceAdded(String title) {
    return 'Indexed $title';
  }

  @override
  String knowledgeAttachUnsupported(String kind) {
    return '$kind files cannot be indexed in this build';
  }

  @override
  String get knowledgeNoSources => 'No sources indexed yet.';

  @override
  String knowledgeRemoved(String title) {
    return 'Removed $title';
  }

  @override
  String get agentsUnavailableBody =>
      'Agent orchestration is not enabled in this release. The built-in skill families (Skills tab) are available today.';

  @override
  String get modelInstallAction => 'Install';

  @override
  String get modelInstalling => 'Installing…';

  @override
  String get modelInstallFailed => 'Install failed';

  @override
  String get modelInstallCancelled => 'Install cancelled';

  @override
  String get modelsHfSearchFailed =>
      'Search failed — the network refused the query. Try again shortly.';

  @override
  String get importModelTooltip => 'Install a local GGUF file';

  @override
  String importModelInstalled(String package) {
    return 'Installed $package';
  }

  @override
  String get importModelFailed => 'Local install failed';

  @override
  String get settingsIdentity => 'Device identity';

  @override
  String get settingsWorkspaceId => 'Workspace';

  @override
  String get opResolving => 'Resolving package…';

  @override
  String opDownloading(int done, int total) {
    return '$done of $total';
  }

  @override
  String get opVerifying => 'Verifying hashes…';

  @override
  String get opInstalling => 'Installing…';

  @override
  String get opLoadingModel => 'Loading model…';

  @override
  String get opGenerating => 'Generating…';

  @override
  String get opIngesting => 'Indexing…';

  @override
  String opBytesMib(int done, int total) {
    return '$done of $total MiB';
  }

  @override
  String get appStarting => 'Starting the local core…';

  @override
  String get coreStartFailed =>
      'The native core could not be loaded. Harbor runs degraded without it.';

  @override
  String get knowledgeIngestFailed => 'Indexing failed';

  @override
  String get fileGroupDocuments => 'Documents';
}

// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for Spanish Castilian (`es`).
class AppLocalizationsEs extends AppLocalizations {
  AppLocalizationsEs([String locale = 'es']) : super(locale);

  @override
  String get surfaceHome => 'Home';

  @override
  String get surfaceAsk => 'Ask';

  @override
  String get surfaceWork => 'Trabajo';

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
  String get surfaceSettings => 'Ajustes';

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
  String get workEmptyTitle => 'Tu espacio de trabajo está vacío';

  @override
  String get workEmptyBody =>
      'Abre un archivo o crea uno nuevo — todo permanece en este dispositivo.';

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
  String get settingsLanguage => 'Idioma';

  @override
  String get settingsEnglish => 'Inglés';

  @override
  String get settingsArabic => 'Árabe';

  @override
  String get settingsTheme => 'Tema';

  @override
  String get settingsThemeLight => 'Claro';

  @override
  String get settingsThemeDark => 'Oscuro';

  @override
  String get settingsPrivacy => 'Privacidad';

  @override
  String get agentsEmptyTitle => 'No agents configured';

  @override
  String get agentsEmptyBody =>
      'Agents combine a model, tools, skills, knowledge and policy. Create one to delegate multi-step work with durable, inspectable runs.';

  @override
  String get skillsEmptyTitle => 'Built-in skills loaded';

  @override
  String get skillsEmptyBody =>
      'Skill definitions for documents, spreadsheets, research, translation and more. Runnable ones carry an executable graph.';

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
    return 'La edición completa requiere $min px de ancho.';
  }

  @override
  String get canvasViewportEditing => 'Edición adaptada a la pantalla';

  @override
  String get openFile => 'Abrir archivo';

  @override
  String sheetLabel(String name) {
    return 'Hoja';
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
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count tools',
      one: '1 tool',
    );
    return '$_temp0';
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
  String get lensOpenTooltip => 'Abrir panel';

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
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count chunks',
      one: '1 chunk',
    );
    return '$_temp0';
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
  String get settingsIdentity => 'Identidad del espacio';

  @override
  String get settingsWorkspaceId => 'Identificador';

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
  String get appStarting => 'Iniciando…';

  @override
  String get coreStartFailed =>
      'El motor no pudo iniciarse en este dispositivo.';

  @override
  String get knowledgeIngestFailed => 'Indexing failed';

  @override
  String get fileGroupDocuments => 'Documentos';

  @override
  String get navMore => 'More';

  @override
  String get navMoreTitle => 'All surfaces';

  @override
  String get lensTitle => 'Harbor Lens';

  @override
  String get lensSubtitle => 'Context, runs and knowledge';

  @override
  String get lensToggleTooltip => 'Show or hide the Harbor Lens';

  @override
  String get closeAction => 'Close';

  @override
  String get lensSectionTrust => 'Trust Pulse';

  @override
  String get lensSectionOps => 'Background work';

  @override
  String get lensSectionRuns => 'Recent runs';

  @override
  String get lensSectionModel => 'Active model';

  @override
  String get lensSectionKnowledge => 'Knowledge index';

  @override
  String get viewAllAction => 'View all';

  @override
  String get trustPolicyHeading => 'Workspace policy';

  @override
  String get trustExecutionHeading => 'Current execution';

  @override
  String trustPolicyVersion(String version) {
    return 'Policy version $version';
  }

  @override
  String get trustLocalOnlyBody =>
      'Los archivos nunca salen de este dispositivo. La suite no realiza conexiones en línea.';

  @override
  String get trustChipLabel => 'LOCAL';

  @override
  String get trustChipTooltip =>
      'Local Only policy · executing on device. Open the Trust Pulse.';

  @override
  String get commandPaletteTooltip => 'Search commands';

  @override
  String get commandPaletteHint => 'Go to a surface or run an action…';

  @override
  String get commandPaletteEmpty => 'No matching commands';

  @override
  String commandGoTo(String surface) {
    return 'Go to $surface';
  }

  @override
  String get commandSectionSurfaces => 'Surfaces';

  @override
  String get commandSectionActions => 'Actions';

  @override
  String get commandToggleLens => 'Toggle Harbor Lens';

  @override
  String get commandToggleTheme => 'Switch light/dark theme';

  @override
  String get commandSwitchLanguage => 'Switch language';

  @override
  String get coreDegradedTitle => 'Motor local no disponible';

  @override
  String get appTagline => 'Your AI. Your models. Your device. Your work.';

  @override
  String get surfaceTitleHome => 'Home';

  @override
  String get homeGreetingMorning => 'Good morning';

  @override
  String get homeGreetingAfternoon => 'Good afternoon';

  @override
  String get homeGreetingEvening => 'Good evening';

  @override
  String get homeModelHeading => 'Active model';

  @override
  String get homeSectionActive => 'In progress';

  @override
  String get homeSectionRecent => 'Recent runs';

  @override
  String get homeRecentEmpty => 'Runs you start appear here with their state.';

  @override
  String knowledgeSourcesCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count sources',
      one: '1 source',
    );
    return '$_temp0';
  }

  @override
  String get homeKnowledgeNotOpen => 'Index not open';

  @override
  String get homeOpenActivity => 'Open Activity';

  @override
  String get homeRequestQueued => 'Request recorded as a durable run';

  @override
  String get homeRequestFailed => 'The request could not be recorded';

  @override
  String get quickActionsHeading => 'Quick actions';

  @override
  String get askSubtitle =>
      'Grounded answers with citations from your Knowledge';

  @override
  String get askYou => 'You';

  @override
  String get askEvidenceHeading => 'Evidence';

  @override
  String get askRetrievalOnlyNote => 'Retrieval only — no chat model was used.';

  @override
  String get askCancelledTitle => 'Generation cancelled';

  @override
  String get askCancelledBody => 'Nothing was recorded for this question.';

  @override
  String get askModelPicker => 'Chat model';

  @override
  String get askClearConversation => 'Clear conversation';

  @override
  String get askComposerHint => 'Ask about your files…';

  @override
  String get askErrorTitle => 'Generation failed';

  @override
  String get askGroundedBadge => 'GROUNDED';

  @override
  String get askUngroundedBadge => 'NOT GROUNDED';

  @override
  String get workSubtitleEmpty => 'Abre un documento para empezar.';

  @override
  String get workPreviewOnly => 'Vista previa';

  @override
  String get workPreviewOnlyBody =>
      'Solo lectura para este tipo; la edición admite hojas de cálculo y documentos.';

  @override
  String get workCloseFile => 'Cerrar archivo';

  @override
  String get workOpening => 'Abriendo…';

  @override
  String get workOpenFailedTitle => 'Error al abrir';

  @override
  String get workOpenFailedBody => 'No se pudo abrir el archivo.';

  @override
  String get workCompatibilityTitle => 'Informe de compatibilidad';

  @override
  String workCompatibilityBody(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count partes se conservan sin mostrarse. No se ejecuta nada.',
      one: '1 parte se conserva sin mostrarse. No se ejecuta nada.',
    );
    return '$_temp0';
  }

  @override
  String get workCompatibilityShow => 'Detalles de compatibilidad';

  @override
  String get workCompatibilityHide => 'Ocultar detalles';

  @override
  String get workOutline => 'Esquema';

  @override
  String get workOutlineEmpty => 'No headings';

  @override
  String workParagraphs(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count párrafos',
      one: '1 párrafo',
    );
    return '$_temp0';
  }

  @override
  String workPages(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count páginas',
      one: '1 página',
    );
    return '$_temp0';
  }

  @override
  String workSlides(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count diapositivas',
      one: '1 diapositiva',
    );
    return '$_temp0';
  }

  @override
  String workCells(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count celdas',
      one: '1 celda',
    );
    return '$_temp0';
  }

  @override
  String workCharts(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count gráficos',
      one: '1 gráfico',
    );
    return '$_temp0';
  }

  @override
  String get workFormulaBar => 'Formula';

  @override
  String get workValue => 'Valor';

  @override
  String get workCellUnverified => 'valor en caché — sin verificar';

  @override
  String get workSheetNotPreviewed =>
      'Only the first sheet is previewed in this build';

  @override
  String workPage(int index) {
    return 'Página $index';
  }

  @override
  String workSlide(int index) {
    return 'Diapositiva $index';
  }

  @override
  String get workKindDocument => 'Documento';

  @override
  String get workKindWorkbook => 'Hoja de cálculo';

  @override
  String get workKindDeck => 'Presentación';

  @override
  String get workKindPdf => 'PDF';

  @override
  String get workSupportedTypes => 'Tipos compatibles';

  @override
  String get workEmptyTextPage => 'No se extrajo texto en esta página';

  @override
  String get workShowFormulas => 'Mostrar fórmulas';

  @override
  String get workNoSelection => 'Select a cell';

  @override
  String get modelsSubtitle =>
      'Install, inspect and choose what runs on this device';

  @override
  String get modelsImportBody =>
      'Install a local GGUF file. Packages are data only — no repository code ever executes.';

  @override
  String get modelsUseForAsk => 'Use for Ask';

  @override
  String get modelsInUse => 'In use';

  @override
  String get modelsRuntime => 'Runtime';

  @override
  String get modelsFiles => 'Files';

  @override
  String get modelsSize => 'Size';

  @override
  String get modelsRecommendedBody =>
      'Recommendations are ranked by Fit Score once the signed catalog is synced. Until then, search Hugging Face or import a local package.';

  @override
  String get modelsHfSearchHint => 'Search public repositories';

  @override
  String modelsDownloads(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count downloads',
      one: '1 download',
    );
    return '$_temp0';
  }

  @override
  String modelsLikes(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count likes',
      one: '1 like',
    );
    return '$_temp0';
  }

  @override
  String get modelsAcquireTitle => 'Acquiring model';

  @override
  String modelsInstalledCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count installed',
      one: '1 installed',
    );
    return '$_temp0';
  }

  @override
  String get modelsGoHuggingFace => 'Search Hugging Face';

  @override
  String get modelsImportGguf => 'Import GGUF';

  @override
  String get modelsFitComputing => 'Computing Fit Score…';

  @override
  String get fitLabelExcellent => 'Excellent';

  @override
  String get fitLabelGood => 'Good';

  @override
  String get fitLabelLimited => 'Limited';

  @override
  String get fitLabelTooLarge => 'Too large';

  @override
  String get fitLabelUnsupported => 'Unsupported';

  @override
  String get fileGroupModels => 'Model packages';

  @override
  String get agentsSubtitle =>
      'Profiles that combine a model, tools, skills, knowledge and policy';

  @override
  String get agentsUnavailableTitle => 'Not enabled in this release';

  @override
  String get agentsWhatTitle => 'What an agent profile will contain';

  @override
  String get agentsPartModel => 'Model';

  @override
  String get agentsPartModelBody =>
      'A qualified installed model with an explicit, sticky lock per workspace.';

  @override
  String get agentsPartTools => 'Tools';

  @override
  String get agentsPartToolsBody =>
      'An allowlist drawn from the built-in skill families.';

  @override
  String get agentsPartKnowledge => 'Knowledge';

  @override
  String get agentsPartKnowledgeBody =>
      'Collections the agent may cite, never sources it was not granted.';

  @override
  String get agentsPartPolicy => 'Policy & approvals';

  @override
  String get agentsPartPolicyBody =>
      'Protected effects pause for a Harbor Sheet review before anything is written.';

  @override
  String get agentsGoSkills => 'Browse skills';

  @override
  String get agentsGoActivity => 'View runs';

  @override
  String get skillsSubtitle =>
      'Built-in skill definitions. Graph skills run through the durable executor; prose skills are declarations until decomposed.';

  @override
  String get skillsSearchHint => 'Filter skills';

  @override
  String skillsCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count skills',
      one: '1 skill',
    );
    return '$_temp0';
  }

  @override
  String get skillsToolsHeading => 'Tools';

  @override
  String get skillsNoMatch => 'No skills match the filter';

  @override
  String get skillsFamily => 'Family';

  @override
  String get skillsBuiltIn => 'Built-in';

  @override
  String get knowledgeSubtitle =>
      'A local, citation-backed index. Sources never leave the device.';

  @override
  String get knowledgeIndexHeading => 'Index';

  @override
  String get knowledgeIdentity => 'Identity';

  @override
  String get knowledgeDimension => 'Dimensions';

  @override
  String get knowledgeEmbedding => 'Embedding model';

  @override
  String knowledgeRemoveConfirmTitle(String title) {
    return 'Remove $title?';
  }

  @override
  String get knowledgeRemoveConfirmBody =>
      'Its chunks leave the index. Past citations will show the source as removed.';

  @override
  String get knowledgeOpening => 'Opening the index…';

  @override
  String get knowledgeGoModels => 'Open Models';

  @override
  String knowledgeSourceKb(int kb) {
    return '$kb KB';
  }

  @override
  String get activitySubtitle => 'Durable runs and background operations';

  @override
  String get activityTabRuns => 'Runs';

  @override
  String get activityTabOps => 'Operations';

  @override
  String get activityOpsEmptyTitle => 'No background operations';

  @override
  String get activityOpsEmptyBody =>
      'Downloads, indexing and generation appear here while they run and after they finish.';

  @override
  String get activityRunDetail => 'Run detail';

  @override
  String get activityFinalState => 'Final state';

  @override
  String get activityVerifiedEvents => 'Verified events';

  @override
  String get activityTrailHeading => 'Run Trail';

  @override
  String activityRunsCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count runs',
      one: '1 run',
      zero: 'No runs',
    );
    return '$_temp0';
  }

  @override
  String get activityReplayFailed => 'The run could not be replayed';

  @override
  String get activityOpKindAcquire => 'Model acquisition';

  @override
  String get activityOpKindIngest => 'Knowledge indexing';

  @override
  String get activityOpKindGenerate => 'Grounded generation';

  @override
  String get settingsSubtitle => 'Idioma, tema y privacidad';

  @override
  String get settingsThemeSystem => 'Sistema';

  @override
  String get settingsLanguageBody =>
      'La interfaz sigue el idioma elegido; el árabe se muestra de derecha a izquierda.';

  @override
  String get settingsPrivacyBody =>
      'Solo local es la política predeterminada y única de esta versión.';

  @override
  String get settingsAbout => 'About';

  @override
  String get settingsVersion => 'Version';

  @override
  String get settingsCoreStatus => 'Native core';

  @override
  String get settingsCoreLoaded => 'Loaded';

  @override
  String get settingsCoreDegraded => 'Not loaded — degraded';

  @override
  String get settingsShortcuts => 'Keyboard shortcuts';

  @override
  String get settingsShortcutSurfaces => 'Switch surfaces';

  @override
  String get settingsShortcutPalette => 'Command palette';

  @override
  String get settingsLensDocked => 'Dock the Lens on wide windows';

  @override
  String get settingsMotionNote => 'El tema sigue los ajustes del sistema.';

  @override
  String get copyAction => 'Copiar';

  @override
  String get copiedMessage => 'Copiado';

  @override
  String get activityExecutorTime => 'Executor time';

  @override
  String get activityStepsLabel => 'Steps';

  @override
  String get skillsRunnable => 'Runnable graph';

  @override
  String get skillsDeclaration => 'Declaration only';

  @override
  String get skillsDeclarationBody =>
      'This skill is a prose declaration: it has no executable graph yet, so nothing runs it. Its instructions and allowlist are the spec for a future graph.';

  @override
  String get skillsGraphHeading => 'Graph';

  @override
  String skillsGraphNodes(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count nodes',
      one: '1 node',
    );
    return '$_temp0';
  }

  @override
  String skillsGraphModelNodes(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count model nodes, schema-constrained',
      one: '1 model node, schema-constrained',
      zero: 'No model calls — fully deterministic',
    );
    return '$_temp0';
  }

  @override
  String skillsGraphBudgets(int steps, int tools) {
    return 'Budget: $steps steps, $tools tool calls';
  }

  @override
  String get skillsRun => 'Run';

  @override
  String skillsRunTitle(String title) {
    return 'Run $title';
  }

  @override
  String get skillsAttachFile => 'Attach file';

  @override
  String skillsAttached(String name) {
    return 'Attached: $name';
  }

  @override
  String get skillsValuesHint => 'One value per line: key = value';

  @override
  String get skillsNeedsModel =>
      'This skill has model nodes. Install a chat model in Models to run it; the model only fills typed slots.';

  @override
  String get skillsModelLabel => 'Model';

  @override
  String get skillsRunning => 'Running on device…';

  @override
  String get skillsRunFailed => 'Run failed';

  @override
  String get skillsOutcome => 'Outcome';

  @override
  String get skillsOutcomeCompleted => 'Completed';

  @override
  String get skillsOutcomeNeedsInput => 'Needs input — nothing was invented';

  @override
  String get skillsOutcomeAbstained => 'Abstained';

  @override
  String get skillsRunState => 'Run state';

  @override
  String get skillsTrailHeading => 'Node trail';

  @override
  String get skillsApprovalTitle => 'Approval required';

  @override
  String skillsApprovalBody(String effect, int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count operations',
      one: '1 operation',
    );
    return 'The run proposed a $effect affecting $_temp0. Nothing has been written. The proposal is bound to the base content hash and the hash of the output it would produce.';
  }

  @override
  String get skillsApprove => 'Approve';

  @override
  String get skillsReject => 'Reject';

  @override
  String get skillsBaseHash => 'Base content hash';

  @override
  String get skillsProposedHash => 'Proposed output hash';

  @override
  String get skillsOutputsHeading => 'Outputs';

  @override
  String get skillsRequiredField => 'Required';

  @override
  String get skillsCancelRun => 'Cancel run';

  @override
  String get skillsInputsHeading => 'Inputs';

  @override
  String skillsExecutedOn(String model) {
    return 'Executed on $model';
  }

  @override
  String skillsStructuredMode(String mode) {
    return 'Structured output: $mode';
  }

  @override
  String skillsCommitBody(String effect, int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count operations',
      one: '1 operation',
    );
    return 'The run proposed a $effect affecting $_temp0. Nothing has been written yet. Save new copy writes the approved output as a new file; the original is never modified. The proposal is bound to the base content hash and the hash of the output it produces.';
  }

  @override
  String get skillsSaveNewCopy => 'Save new copy';

  @override
  String get skillsOverwrite => 'Overwrite original…';

  @override
  String get skillsOverwriteConfirmTitle => 'Overwrite the original?';

  @override
  String skillsOverwriteConfirmBody(String name) {
    return 'Harbor will replace $name in place. This only happens if the file still matches the approved base; otherwise nothing is written and the run stops with the reason.';
  }

  @override
  String get skillsSavedNewCopy => 'Saved as a new copy';

  @override
  String get skillsOverwritten => 'Original replaced';

  @override
  String skillsCommittedTo(String path, String version) {
    return '$path · version $version';
  }

  @override
  String get skillsDiffBase => 'base';

  @override
  String get skillsDiffProposed => 'proposed';

  @override
  String get settingsDiagnostics => 'Diagnóstico';

  @override
  String get settingsDiagnosticsBody =>
      'Registro cifrado de errores; solo exportación manual.';

  @override
  String get settingsDiagnosticsContains => 'Contiene';

  @override
  String settingsDiagnosticsRecords(int count) {
    return 'entradas';
  }

  @override
  String get settingsDiagnosticsExport => 'Exportar registro';

  @override
  String get settingsDiagnosticsExported => 'Registro exportado';

  @override
  String settingsDiagnosticsExportedBody(String path, int count) {
    return 'El registro cifrado se guardó donde elegiste.';
  }

  @override
  String get settingsDiagnosticsExportFailed => 'Error al exportar';

  @override
  String get homeFirstRunTitle => 'Install a model to get started';

  @override
  String get homeFirstRunBody =>
      'Harbor runs entirely on this device — nothing you open leaves it. Every answer, skill and index needs a local model, so the first step is choosing one that fits your device.';

  @override
  String get homeFirstRunAction => 'Choose a model';

  @override
  String get modelsFirstRunTitle => 'Local only';

  @override
  String get modelsFirstRunBody =>
      'These packages come from Harbor\'s signed catalog. Check size & fit reads the file list from Hugging Face through the broker; Install downloads once and verifies the pinned hash. Nothing else leaves the device.';

  @override
  String modelsCatalogHeading(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count catalog packages',
      one: '1 catalog package',
    );
    return '$_temp0';
  }

  @override
  String get modelsCatalogFooter =>
      'Fit Score is computed by the core from this device\'s memory and accelerator; it is never guessed. Sizes come from the repository listing, not the catalog.';

  @override
  String get modelsCatalogCheckFit => 'Check size & fit';

  @override
  String get modelsCatalogInstall => 'Install';

  @override
  String get modelsCatalogSizeUnavailable =>
      'Size unavailable (the repository listing could not be read).';

  @override
  String get modelsInstalledBadge => 'Installed';

  @override
  String modelsContextTokens(int count) {
    return '$count tokens context';
  }

  @override
  String get opAcquireRunning => 'Installing…';

  @override
  String get modelsCatalogDoesNotFit => 'Does not fit this device';

  @override
  String skillsCreateBody(String name, int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count operations',
      one: '1 operation',
    );
    return 'The run proposed a new file, $name, built from $_temp0. Nothing has been written yet. Save new file writes it where you choose; nothing else on this device changes. The proposal is bound to the hash of the file it produces.';
  }

  @override
  String get skillsSaveNewFile => 'Save new file';

  @override
  String get skillsSavedNewFile => 'Saved as a new file';

  @override
  String get skillsCopy => 'Copy';

  @override
  String get skillsCopied => 'Copied to the clipboard';

  @override
  String get surfaceStore => 'Store';

  @override
  String get storeTagline => 'Included products and first-party models';

  @override
  String get storeOfficeTitle => 'Harbor Office Suite';

  @override
  String get storeOfficeIncluded => 'Included with this app';

  @override
  String get storeOfficeStatus =>
      'Spreadsheet editing, charts and highlights are live in Work; formula targets qualify as their platform gates pass.';

  @override
  String get storeOfficeOpen => 'Open in Work';

  @override
  String storeModelsHeading(int count) {
    return 'Harbor models ($count)';
  }

  @override
  String get storeModelsBody =>
      'Signed, first-party catalog entries. Downloads are quoted and confirmed before transfer.';

  @override
  String get storeInstalledHeading => 'Installed on this device';

  @override
  String get storeUninstall => 'Uninstall';

  @override
  String storeUninstallTitle(String id) {
    return 'Uninstall $id?';
  }

  @override
  String storeUninstallBody(String bytes, int files) {
    String _temp0 = intl.Intl.pluralLogic(
      files,
      locale: localeName,
      other: '$files files',
      one: '1 file',
    );
    return 'Removes $bytes across $_temp0. Undo stays available for 72 hours.';
  }

  @override
  String storeUninstallInUse(String reasons) {
    return 'In use: $reasons';
  }

  @override
  String storeUninstallDone(String id) {
    return 'Uninstalled $id';
  }

  @override
  String get storeUndo => 'Undo';

  @override
  String get storeUndoFailed => 'Restore failed';

  @override
  String get storeAcquireTitle => 'Confirm download';

  @override
  String storeAcquireBody(String quoted, String available) {
    return '$quoted to download; $available free on this device.';
  }

  @override
  String get storeAcquireNoFit => 'Not enough free space for this download.';

  @override
  String get storeAcquireConfirm => 'Download';

  @override
  String get storeUninstallFailed => 'Uninstall failed';

  @override
  String get storePreviewFailed => 'Could not read the package';

  @override
  String get storeEmpty =>
      'The accepted signed catalog is empty on this device.';

  @override
  String get storeManageHint =>
      'Uninstall frees every owned file; a preview shows the exact scope before anything moves.';

  @override
  String get workEditHint => 'Escribe un valor o =fórmula';

  @override
  String get workEditCommit => 'Confirmar celda';

  @override
  String get workBoldRow => 'Poner fila en negrita';

  @override
  String get workAddChart => 'Insertar gráfico desde la columna';

  @override
  String get workConvertMarkdown => 'Convertir Markdown / PDF…';

  @override
  String workConvertedSaved(String path) {
    return 'Guardado: $path';
  }

  @override
  String get workConvertFailed => 'Error al convertir';

  @override
  String get modelsAdvancedDetails => 'Advanced details';

  @override
  String get modelsBackendLabel => 'Backend';

  @override
  String get workAddHighlight => 'Resaltar por encima de la selección';

  @override
  String get storeOfficeInstalled => 'Installed';

  @override
  String get storeOfficeShips =>
      'Part of the app you already have — nothing to download.';

  @override
  String get storeOfficeInside => 'What\'s inside';

  @override
  String get storeSuiteSheets => 'Spreadsheets — editing, charts, highlights';

  @override
  String get storeSuiteConvert => 'Conversions — Markdown / PDF to Word';

  @override
  String get storeSuiteFormulas =>
      'Formulas — 99 targets across 3 qualified tiers';

  @override
  String get storeSuiteDocs => 'Documents — preview and preservation';

  @override
  String get storeSuiteSlides => 'Presentations — preview';

  @override
  String get storeSuitePdf => 'PDF — preview and extraction';

  @override
  String get suiteStatusLive => 'Live';

  @override
  String get suiteStatusPreview => 'Preview';

  @override
  String get workNewSpreadsheet => 'Hoja nueva';

  @override
  String get workNewDocument => 'Documento nuevo';

  @override
  String get workRecents => 'Recientes';

  @override
  String get workUntitledSheet => 'Hoja sin título';

  @override
  String get workUntitledDoc => 'Documento sin título';

  @override
  String workRecentFailed(String name) {
    return 'No se pudo reabrir $name — el sistema ya no concede acceso a esa ubicación.';
  }

  @override
  String workEditParagraphTitle(int index) {
    return 'Editar párrafo $index';
  }

  @override
  String workOpenUnsupported(String name) {
    return 'Tipo de archivo no compatible.';
  }

  @override
  String workOpenUnreadable(String name) {
    return 'Este archivo no se puede leer.';
  }

  @override
  String get workUndo => 'Deshacer';

  @override
  String get workRedo => 'Rehacer';

  @override
  String get workSaveCopy => 'Guardar copia';

  @override
  String workDraftRestored(String name) {
    return 'Se restauraron tus cambios sin guardar: $name';
  }

  @override
  String workSavedCopy(String name) {
    return 'Guardado: $name';
  }

  @override
  String get workClearRecents => 'Borrar';

  @override
  String get workExportPdf => 'Exportar a PDF';

  @override
  String get workInsertRow => 'Insertar fila arriba';

  @override
  String get workDeleteRow => 'Eliminar fila';

  @override
  String get workInsertCol => 'Insertar columna a la izquierda';

  @override
  String get workDeleteCol => 'Eliminar columna';

  @override
  String get workStyle => 'Estilo';

  @override
  String workExportedPdf(String name) {
    return 'Exportado: $name';
  }

  @override
  String get workStructure => 'Filas y columnas';

  @override
  String get workStyleKeep => 'Mantener';

  @override
  String get workStyleH1 => 'Encabezado 1';

  @override
  String get workStyleH2 => 'Encabezado 2';

  @override
  String get workStyleH3 => 'Encabezado 3';

  @override
  String get workStyleBullet => 'Lista con viñetas';

  @override
  String get workStyleNumbered => 'Lista numerada';

  @override
  String get workStyleNormal => 'Texto normal';

  @override
  String get workConvertTo => 'Convertir Markdown a…';

  @override
  String get workMdFormatBody =>
      '¿Documento Word (texto completo) o presentación PowerPoint (una diapositiva por título)?';

  @override
  String get workMdFormatWord => 'Word';

  @override
  String get workMdFormatSlides => 'Diapositivas';

  @override
  String workEditSlideTitle(int index) {
    return 'Editar título de la diapositiva $index';
  }

  @override
  String workEditSlideBullets(int index) {
    return 'Editar viñetas de la diapositiva $index (una por línea)';
  }

  @override
  String get workPrint => 'Imprimir';

  @override
  String get workPrintUnavailable =>
      'Aquí no hay impresión — usa Exportar a PDF.';
}

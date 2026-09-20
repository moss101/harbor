import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:intl/intl.dart' as intl;

import 'app_localizations_ar.dart';
import 'app_localizations_en.dart';

// ignore_for_file: type=lint

/// Callers can lookup localized strings with an instance of AppLocalizations
/// returned by `AppLocalizations.of(context)`.
///
/// Applications need to include `AppLocalizations.delegate()` in their app's
/// `localizationDelegates` list, and the locales they support in the app's
/// `supportedLocales` list. For example:
///
/// ```dart
/// import 'l10n/app_localizations.dart';
///
/// return MaterialApp(
///   localizationsDelegates: AppLocalizations.localizationsDelegates,
///   supportedLocales: AppLocalizations.supportedLocales,
///   home: MyApplicationHome(),
/// );
/// ```
///
/// ## Update pubspec.yaml
///
/// Please make sure to update your pubspec.yaml to include the following
/// packages:
///
/// ```yaml
/// dependencies:
///   # Internationalization support.
///   flutter_localizations:
///     sdk: flutter
///   intl: any # Use the pinned version from flutter_localizations
///
///   # Rest of dependencies
/// ```
///
/// ## iOS Applications
///
/// iOS applications define key application metadata, including supported
/// locales, in an Info.plist file that is built into the application bundle.
/// To configure the locales supported by your app, you’ll need to edit this
/// file.
///
/// First, open your project’s ios/Runner.xcworkspace Xcode workspace file.
/// Then, in the Project Navigator, open the Info.plist file under the Runner
/// project’s Runner folder.
///
/// Next, select the Information Property List item, select Add Item from the
/// Editor menu, then select Localizations from the pop-up menu.
///
/// Select and expand the newly-created Localizations item then, for each
/// locale your application supports, add a new item and select the locale
/// you wish to add from the pop-up menu in the Value field. This list should
/// be consistent with the languages listed in the AppLocalizations.supportedLocales
/// property.
abstract class AppLocalizations {
  AppLocalizations(String locale)
      : localeName = intl.Intl.canonicalizedLocale(locale.toString());

  final String localeName;

  static AppLocalizations? of(BuildContext context) {
    return Localizations.of<AppLocalizations>(context, AppLocalizations);
  }

  static const LocalizationsDelegate<AppLocalizations> delegate =
      _AppLocalizationsDelegate();

  /// A list of this localizations delegate along with the default localizations
  /// delegates.
  ///
  /// Returns a list of localizations delegates containing this delegate along with
  /// GlobalMaterialLocalizations.delegate, GlobalCupertinoLocalizations.delegate,
  /// and GlobalWidgetsLocalizations.delegate.
  ///
  /// Additional delegates can be added by appending to this list in
  /// MaterialApp. This list does not have to be used at all if a custom list
  /// of delegates is preferred or required.
  static const List<LocalizationsDelegate<dynamic>> localizationsDelegates =
      <LocalizationsDelegate<dynamic>>[
    delegate,
    GlobalMaterialLocalizations.delegate,
    GlobalCupertinoLocalizations.delegate,
    GlobalWidgetsLocalizations.delegate,
  ];

  /// A list of this localizations delegate's supported locales.
  static const List<Locale> supportedLocales = <Locale>[
    Locale('ar'),
    Locale('en')
  ];

  /// No description provided for @surfaceHome.
  ///
  /// In en, this message translates to:
  /// **'Home'**
  String get surfaceHome;

  /// No description provided for @surfaceAsk.
  ///
  /// In en, this message translates to:
  /// **'Ask'**
  String get surfaceAsk;

  /// No description provided for @surfaceWork.
  ///
  /// In en, this message translates to:
  /// **'Work'**
  String get surfaceWork;

  /// No description provided for @surfaceAgents.
  ///
  /// In en, this message translates to:
  /// **'Agents'**
  String get surfaceAgents;

  /// No description provided for @surfaceModels.
  ///
  /// In en, this message translates to:
  /// **'Models'**
  String get surfaceModels;

  /// No description provided for @surfaceSkills.
  ///
  /// In en, this message translates to:
  /// **'Skills'**
  String get surfaceSkills;

  /// No description provided for @surfaceKnowledge.
  ///
  /// In en, this message translates to:
  /// **'Knowledge'**
  String get surfaceKnowledge;

  /// No description provided for @surfaceActivity.
  ///
  /// In en, this message translates to:
  /// **'Activity'**
  String get surfaceActivity;

  /// No description provided for @surfaceSettings.
  ///
  /// In en, this message translates to:
  /// **'Settings'**
  String get surfaceSettings;

  /// No description provided for @homeHeadline.
  ///
  /// In en, this message translates to:
  /// **'What do you want to get done?'**
  String get homeHeadline;

  /// No description provided for @homeComposerHint.
  ///
  /// In en, this message translates to:
  /// **'Describe the work, attach files, and go.'**
  String get homeComposerHint;

  /// No description provided for @quickSummarize.
  ///
  /// In en, this message translates to:
  /// **'Summarize document'**
  String get quickSummarize;

  /// No description provided for @quickAnalyze.
  ///
  /// In en, this message translates to:
  /// **'Analyze spreadsheet'**
  String get quickAnalyze;

  /// No description provided for @quickPresent.
  ///
  /// In en, this message translates to:
  /// **'Create presentation'**
  String get quickPresent;

  /// No description provided for @quickCompare.
  ///
  /// In en, this message translates to:
  /// **'Compare files'**
  String get quickCompare;

  /// No description provided for @quickOrganize.
  ///
  /// In en, this message translates to:
  /// **'Organize project'**
  String get quickOrganize;

  /// No description provided for @quickResearch.
  ///
  /// In en, this message translates to:
  /// **'Research this folder'**
  String get quickResearch;

  /// No description provided for @quickTranslate.
  ///
  /// In en, this message translates to:
  /// **'Translate content'**
  String get quickTranslate;

  /// No description provided for @workEmptyTitle.
  ///
  /// In en, this message translates to:
  /// **'No artifact open'**
  String get workEmptyTitle;

  /// No description provided for @workEmptyBody.
  ///
  /// In en, this message translates to:
  /// **'Open a document, workbook, deck or PDF to see it here with version, verification and conflict state.'**
  String get workEmptyBody;

  /// No description provided for @modelsInstalled.
  ///
  /// In en, this message translates to:
  /// **'Installed'**
  String get modelsInstalled;

  /// No description provided for @modelsRecommended.
  ///
  /// In en, this message translates to:
  /// **'Recommended'**
  String get modelsRecommended;

  /// No description provided for @modelsLibrary.
  ///
  /// In en, this message translates to:
  /// **'Harbor Library'**
  String get modelsLibrary;

  /// No description provided for @modelsHuggingFace.
  ///
  /// In en, this message translates to:
  /// **'Hugging Face'**
  String get modelsHuggingFace;

  /// No description provided for @modelsImport.
  ///
  /// In en, this message translates to:
  /// **'Import'**
  String get modelsImport;

  /// No description provided for @modelsBenchmark.
  ///
  /// In en, this message translates to:
  /// **'Benchmark'**
  String get modelsBenchmark;

  /// No description provided for @fitScore.
  ///
  /// In en, this message translates to:
  /// **'Fit Score'**
  String get fitScore;

  /// No description provided for @trustPolicy.
  ///
  /// In en, this message translates to:
  /// **'Policy: LOCAL ONLY'**
  String get trustPolicy;

  /// No description provided for @trustExecutionOnDevice.
  ///
  /// In en, this message translates to:
  /// **'Execution: ON DEVICE'**
  String get trustExecutionOnDevice;

  /// No description provided for @runTrailEmpty.
  ///
  /// In en, this message translates to:
  /// **'No activity yet.'**
  String get runTrailEmpty;

  /// No description provided for @approvalApprove.
  ///
  /// In en, this message translates to:
  /// **'Approve'**
  String get approvalApprove;

  /// No description provided for @approvalDeny.
  ///
  /// In en, this message translates to:
  /// **'Deny'**
  String get approvalDeny;

  /// No description provided for @settingsLanguage.
  ///
  /// In en, this message translates to:
  /// **'Language'**
  String get settingsLanguage;

  /// No description provided for @settingsEnglish.
  ///
  /// In en, this message translates to:
  /// **'English'**
  String get settingsEnglish;

  /// No description provided for @settingsArabic.
  ///
  /// In en, this message translates to:
  /// **'العربية'**
  String get settingsArabic;

  /// No description provided for @settingsTheme.
  ///
  /// In en, this message translates to:
  /// **'Theme'**
  String get settingsTheme;

  /// No description provided for @settingsThemeLight.
  ///
  /// In en, this message translates to:
  /// **'Light'**
  String get settingsThemeLight;

  /// No description provided for @settingsThemeDark.
  ///
  /// In en, this message translates to:
  /// **'Dark'**
  String get settingsThemeDark;

  /// No description provided for @settingsPrivacy.
  ///
  /// In en, this message translates to:
  /// **'Workspace privacy'**
  String get settingsPrivacy;

  /// No description provided for @agentsEmptyTitle.
  ///
  /// In en, this message translates to:
  /// **'No agents configured'**
  String get agentsEmptyTitle;

  /// No description provided for @agentsEmptyBody.
  ///
  /// In en, this message translates to:
  /// **'Agents combine a model, tools, skills, knowledge and policy. Create one to delegate multi-step work with durable, inspectable runs.'**
  String get agentsEmptyBody;

  /// No description provided for @skillsEmptyTitle.
  ///
  /// In en, this message translates to:
  /// **'Built-in skills loaded'**
  String get skillsEmptyTitle;

  /// No description provided for @skillsEmptyBody.
  ///
  /// In en, this message translates to:
  /// **'Skill definitions for documents, spreadsheets, research, translation and more. Runnable ones carry an executable graph.'**
  String get skillsEmptyBody;

  /// No description provided for @knowledgeEmptyTitle.
  ///
  /// In en, this message translates to:
  /// **'Knowledge is empty'**
  String get knowledgeEmptyTitle;

  /// No description provided for @knowledgeEmptyBody.
  ///
  /// In en, this message translates to:
  /// **'Add folders or documents to build a local, citation-backed index. Sources never leave the device under Local Only.'**
  String get knowledgeEmptyBody;

  /// No description provided for @activityEmptyTitle.
  ///
  /// In en, this message translates to:
  /// **'No runs yet'**
  String get activityEmptyTitle;

  /// No description provided for @activityEmptyBody.
  ///
  /// In en, this message translates to:
  /// **'Durable agent runs appear here with their full Run Trail — including pause, approval and recovery states.'**
  String get activityEmptyBody;

  /// No description provided for @askEmptyTitle.
  ///
  /// In en, this message translates to:
  /// **'Ask works on your files'**
  String get askEmptyTitle;

  /// No description provided for @askEmptyBody.
  ///
  /// In en, this message translates to:
  /// **'Answers ground in your workspace with citations and abstain when evidence is insufficient.'**
  String get askEmptyBody;

  /// No description provided for @lensButton.
  ///
  /// In en, this message translates to:
  /// **'Lens'**
  String get lensButton;

  /// No description provided for @workCanvasRuleActive.
  ///
  /// In en, this message translates to:
  /// **'canvas ≥ {min}px rule active'**
  String workCanvasRuleActive(int min);

  /// No description provided for @canvasViewportEditing.
  ///
  /// In en, this message translates to:
  /// **'viewport-sized editing'**
  String get canvasViewportEditing;

  /// No description provided for @openFile.
  ///
  /// In en, this message translates to:
  /// **'Open file'**
  String get openFile;

  /// No description provided for @sheetLabel.
  ///
  /// In en, this message translates to:
  /// **'Sheet: {name}'**
  String sheetLabel(String name);

  /// No description provided for @attachFilesTooltip.
  ///
  /// In en, this message translates to:
  /// **'Attach files'**
  String get attachFilesTooltip;

  /// No description provided for @modelDockEmpty.
  ///
  /// In en, this message translates to:
  /// **'No model installed — open Models to install one'**
  String get modelDockEmpty;

  /// No description provided for @modelDockCoreUnavailable.
  ///
  /// In en, this message translates to:
  /// **'Core unavailable — native runtime not loaded'**
  String get modelDockCoreUnavailable;

  /// No description provided for @modelsLibraryEmpty.
  ///
  /// In en, this message translates to:
  /// **'Curated Harbor library packages appear here.'**
  String get modelsLibraryEmpty;

  /// No description provided for @modelsHfEmpty.
  ///
  /// In en, this message translates to:
  /// **'Search public repositories. Model packages are data — no repository code ever executes.'**
  String get modelsHfEmpty;

  /// No description provided for @coreNotLoadedModels.
  ///
  /// In en, this message translates to:
  /// **'Native core not loaded; installed models are unavailable. Build core/harbor_ffi to enable this view.'**
  String get coreNotLoadedModels;

  /// No description provided for @modelsInstalledEmpty.
  ///
  /// In en, this message translates to:
  /// **'Install a model from Recommended or the Library. Fit Score shows what your device can run well.'**
  String get modelsInstalledEmpty;

  /// No description provided for @modelsBenchmarkEmpty.
  ///
  /// In en, this message translates to:
  /// **'Controlled device-local benchmark workloads with model/runtime/device identity.'**
  String get modelsBenchmarkEmpty;

  /// No description provided for @modelsRecommendedEmpty.
  ///
  /// In en, this message translates to:
  /// **'Recommendations appear once the catalog is synced. A model is recommended only when your device can run it well.'**
  String get modelsRecommendedEmpty;

  /// No description provided for @coreNotLoadedSkills.
  ///
  /// In en, this message translates to:
  /// **'Native core not loaded; skills are declared in the core and cannot be listed.'**
  String get coreNotLoadedSkills;

  /// No description provided for @coreNotLoadedActivity.
  ///
  /// In en, this message translates to:
  /// **'Native core not loaded; durable runs live in the core store and cannot be listed.'**
  String get coreNotLoadedActivity;

  /// No description provided for @newAgent.
  ///
  /// In en, this message translates to:
  /// **'New agent'**
  String get newAgent;

  /// No description provided for @addSources.
  ///
  /// In en, this message translates to:
  /// **'Add sources'**
  String get addSources;

  /// No description provided for @toolsCount.
  ///
  /// In en, this message translates to:
  /// **'{count, plural, one{1 tool} other{{count} tools}}'**
  String toolsCount(int count);

  /// No description provided for @filesSizeRuntime.
  ///
  /// In en, this message translates to:
  /// **'{files} files · {mb} MB · runtime {runtime}'**
  String filesSizeRuntime(int files, int mb, String runtime);

  /// No description provided for @runStateLine.
  ///
  /// In en, this message translates to:
  /// **'state: {state} · {ms} ms executor time'**
  String runStateLine(String state, int ms);

  /// No description provided for @scoreLine.
  ///
  /// In en, this message translates to:
  /// **'score {pct}% · {state}'**
  String scoreLine(String pct, String state);

  /// No description provided for @askNoEvidenceTitle.
  ///
  /// In en, this message translates to:
  /// **'No supporting evidence'**
  String get askNoEvidenceTitle;

  /// No description provided for @askNoEvidenceBody.
  ///
  /// In en, this message translates to:
  /// **'Nothing in the local index supports this question, so I am abstaining rather than guessing.'**
  String get askNoEvidenceBody;

  /// No description provided for @askKnowledgeNotOpen.
  ///
  /// In en, this message translates to:
  /// **'Knowledge is not open yet. Install an embedding model (Models → Installed) to ground answers locally.'**
  String get askKnowledgeNotOpen;

  /// No description provided for @askAbstentionHeading.
  ///
  /// In en, this message translates to:
  /// **'I could not find support for this in your Knowledge.'**
  String get askAbstentionHeading;

  /// No description provided for @sendAction.
  ///
  /// In en, this message translates to:
  /// **'Send'**
  String get sendAction;

  /// No description provided for @lensOpenTooltip.
  ///
  /// In en, this message translates to:
  /// **'Open the Harbor Lens inspector'**
  String get lensOpenTooltip;

  /// No description provided for @askSearchTooltip.
  ///
  /// In en, this message translates to:
  /// **'Search Knowledge'**
  String get askSearchTooltip;

  /// No description provided for @statusInstalled.
  ///
  /// In en, this message translates to:
  /// **'INSTALLED'**
  String get statusInstalled;

  /// No description provided for @cancelAction.
  ///
  /// In en, this message translates to:
  /// **'Cancel'**
  String get cancelAction;

  /// No description provided for @askGenerateTooltip.
  ///
  /// In en, this message translates to:
  /// **'Generate answer'**
  String get askGenerateTooltip;

  /// No description provided for @askAnswerHeading.
  ///
  /// In en, this message translates to:
  /// **'Answer'**
  String get askAnswerHeading;

  /// No description provided for @askCitationsHeading.
  ///
  /// In en, this message translates to:
  /// **'Cited sources'**
  String get askCitationsHeading;

  /// No description provided for @askExecutedOnLine.
  ///
  /// In en, this message translates to:
  /// **'Executed on {model} · {tokens} tokens on-device'**
  String askExecutedOnLine(String model, int tokens);

  /// No description provided for @askGenerating.
  ///
  /// In en, this message translates to:
  /// **'Generating on-device…'**
  String get askGenerating;

  /// No description provided for @askGenerationProgress.
  ///
  /// In en, this message translates to:
  /// **'{tokens} tokens generated'**
  String askGenerationProgress(int tokens);

  /// No description provided for @askInsufficientNote.
  ///
  /// In en, this message translates to:
  /// **'The model reports the evidence is insufficient — this answer is not grounded in your sources.'**
  String get askInsufficientNote;

  /// No description provided for @askNoChatModel.
  ///
  /// In en, this message translates to:
  /// **'No chat model installed. Answers need a chat model (Models → Hugging Face); with Knowledge open, retrieval-only citations still work.'**
  String get askNoChatModel;

  /// No description provided for @askSearchOnly.
  ///
  /// In en, this message translates to:
  /// **'Search Knowledge'**
  String get askSearchOnly;

  /// No description provided for @knowledgeSourcesHeading.
  ///
  /// In en, this message translates to:
  /// **'Indexed sources'**
  String get knowledgeSourcesHeading;

  /// No description provided for @knowledgeChunksCount.
  ///
  /// In en, this message translates to:
  /// **'{count, plural, one{1 chunk} other{{count} chunks}}'**
  String knowledgeChunksCount(int count);

  /// No description provided for @knowledgeRemoveAction.
  ///
  /// In en, this message translates to:
  /// **'Remove'**
  String get knowledgeRemoveAction;

  /// No description provided for @knowledgeIngesting.
  ///
  /// In en, this message translates to:
  /// **'Indexing chunks {done}/{total}'**
  String knowledgeIngesting(int done, int total);

  /// No description provided for @knowledgeOpenNeedsModel.
  ///
  /// In en, this message translates to:
  /// **'Install the embedding model bge-small-en-v1.5 (Models tab) to build the local index.'**
  String get knowledgeOpenNeedsModel;

  /// No description provided for @knowledgeAddFilesTooltip.
  ///
  /// In en, this message translates to:
  /// **'Add files to the local index'**
  String get knowledgeAddFilesTooltip;

  /// No description provided for @knowledgeAddTextAction.
  ///
  /// In en, this message translates to:
  /// **'Paste text'**
  String get knowledgeAddTextAction;

  /// No description provided for @knowledgeAddTextTitle.
  ///
  /// In en, this message translates to:
  /// **'Index pasted text'**
  String get knowledgeAddTextTitle;

  /// No description provided for @knowledgeAddTextTitleHint.
  ///
  /// In en, this message translates to:
  /// **'Title'**
  String get knowledgeAddTextTitleHint;

  /// No description provided for @knowledgeAddTextBodyHint.
  ///
  /// In en, this message translates to:
  /// **'Paste the text to index'**
  String get knowledgeAddTextBodyHint;

  /// No description provided for @knowledgeAddTextConfirm.
  ///
  /// In en, this message translates to:
  /// **'Index'**
  String get knowledgeAddTextConfirm;

  /// No description provided for @knowledgeSourceAdded.
  ///
  /// In en, this message translates to:
  /// **'Indexed {title}'**
  String knowledgeSourceAdded(String title);

  /// No description provided for @knowledgeAttachUnsupported.
  ///
  /// In en, this message translates to:
  /// **'{kind} files cannot be indexed in this build'**
  String knowledgeAttachUnsupported(String kind);

  /// No description provided for @knowledgeNoSources.
  ///
  /// In en, this message translates to:
  /// **'No sources indexed yet.'**
  String get knowledgeNoSources;

  /// No description provided for @knowledgeRemoved.
  ///
  /// In en, this message translates to:
  /// **'Removed {title}'**
  String knowledgeRemoved(String title);

  /// No description provided for @agentsUnavailableBody.
  ///
  /// In en, this message translates to:
  /// **'Agent orchestration is not enabled in this release. The built-in skill families (Skills tab) are available today.'**
  String get agentsUnavailableBody;

  /// No description provided for @modelInstallAction.
  ///
  /// In en, this message translates to:
  /// **'Install'**
  String get modelInstallAction;

  /// No description provided for @modelInstalling.
  ///
  /// In en, this message translates to:
  /// **'Installing…'**
  String get modelInstalling;

  /// No description provided for @modelInstallFailed.
  ///
  /// In en, this message translates to:
  /// **'Install failed'**
  String get modelInstallFailed;

  /// No description provided for @modelInstallCancelled.
  ///
  /// In en, this message translates to:
  /// **'Install cancelled'**
  String get modelInstallCancelled;

  /// No description provided for @modelsHfSearchFailed.
  ///
  /// In en, this message translates to:
  /// **'Search failed — the network refused the query. Try again shortly.'**
  String get modelsHfSearchFailed;

  /// No description provided for @importModelTooltip.
  ///
  /// In en, this message translates to:
  /// **'Install a local GGUF file'**
  String get importModelTooltip;

  /// No description provided for @importModelInstalled.
  ///
  /// In en, this message translates to:
  /// **'Installed {package}'**
  String importModelInstalled(String package);

  /// No description provided for @importModelFailed.
  ///
  /// In en, this message translates to:
  /// **'Local install failed'**
  String get importModelFailed;

  /// No description provided for @settingsIdentity.
  ///
  /// In en, this message translates to:
  /// **'Device identity'**
  String get settingsIdentity;

  /// No description provided for @settingsWorkspaceId.
  ///
  /// In en, this message translates to:
  /// **'Workspace'**
  String get settingsWorkspaceId;

  /// No description provided for @opResolving.
  ///
  /// In en, this message translates to:
  /// **'Resolving package…'**
  String get opResolving;

  /// No description provided for @opDownloading.
  ///
  /// In en, this message translates to:
  /// **'{done} of {total}'**
  String opDownloading(int done, int total);

  /// No description provided for @opVerifying.
  ///
  /// In en, this message translates to:
  /// **'Verifying hashes…'**
  String get opVerifying;

  /// No description provided for @opInstalling.
  ///
  /// In en, this message translates to:
  /// **'Installing…'**
  String get opInstalling;

  /// No description provided for @opLoadingModel.
  ///
  /// In en, this message translates to:
  /// **'Loading model…'**
  String get opLoadingModel;

  /// No description provided for @opGenerating.
  ///
  /// In en, this message translates to:
  /// **'Generating…'**
  String get opGenerating;

  /// No description provided for @opIngesting.
  ///
  /// In en, this message translates to:
  /// **'Indexing…'**
  String get opIngesting;

  /// No description provided for @opBytesMib.
  ///
  /// In en, this message translates to:
  /// **'{done} of {total} MiB'**
  String opBytesMib(int done, int total);

  /// No description provided for @appStarting.
  ///
  /// In en, this message translates to:
  /// **'Starting the local core…'**
  String get appStarting;

  /// No description provided for @coreStartFailed.
  ///
  /// In en, this message translates to:
  /// **'The native core could not be loaded. Harbor runs degraded without it.'**
  String get coreStartFailed;

  /// No description provided for @knowledgeIngestFailed.
  ///
  /// In en, this message translates to:
  /// **'Indexing failed'**
  String get knowledgeIngestFailed;

  /// No description provided for @fileGroupDocuments.
  ///
  /// In en, this message translates to:
  /// **'Documents'**
  String get fileGroupDocuments;

  /// No description provided for @navMore.
  ///
  /// In en, this message translates to:
  /// **'More'**
  String get navMore;

  /// No description provided for @navMoreTitle.
  ///
  /// In en, this message translates to:
  /// **'All surfaces'**
  String get navMoreTitle;

  /// No description provided for @lensTitle.
  ///
  /// In en, this message translates to:
  /// **'Harbor Lens'**
  String get lensTitle;

  /// No description provided for @lensSubtitle.
  ///
  /// In en, this message translates to:
  /// **'Context, runs and knowledge'**
  String get lensSubtitle;

  /// No description provided for @lensToggleTooltip.
  ///
  /// In en, this message translates to:
  /// **'Show or hide the Harbor Lens'**
  String get lensToggleTooltip;

  /// No description provided for @closeAction.
  ///
  /// In en, this message translates to:
  /// **'Close'**
  String get closeAction;

  /// No description provided for @lensSectionTrust.
  ///
  /// In en, this message translates to:
  /// **'Trust Pulse'**
  String get lensSectionTrust;

  /// No description provided for @lensSectionOps.
  ///
  /// In en, this message translates to:
  /// **'Background work'**
  String get lensSectionOps;

  /// No description provided for @lensSectionRuns.
  ///
  /// In en, this message translates to:
  /// **'Recent runs'**
  String get lensSectionRuns;

  /// No description provided for @lensSectionModel.
  ///
  /// In en, this message translates to:
  /// **'Active model'**
  String get lensSectionModel;

  /// No description provided for @lensSectionKnowledge.
  ///
  /// In en, this message translates to:
  /// **'Knowledge index'**
  String get lensSectionKnowledge;

  /// No description provided for @viewAllAction.
  ///
  /// In en, this message translates to:
  /// **'View all'**
  String get viewAllAction;

  /// No description provided for @trustPolicyHeading.
  ///
  /// In en, this message translates to:
  /// **'Workspace policy'**
  String get trustPolicyHeading;

  /// No description provided for @trustExecutionHeading.
  ///
  /// In en, this message translates to:
  /// **'Current execution'**
  String get trustExecutionHeading;

  /// No description provided for @trustPolicyVersion.
  ///
  /// In en, this message translates to:
  /// **'Policy version {version}'**
  String trustPolicyVersion(String version);

  /// No description provided for @trustLocalOnlyBody.
  ///
  /// In en, this message translates to:
  /// **'Requests and files never leave this device. Model acquisition is the only explicit online session, and it runs through the egress broker.'**
  String get trustLocalOnlyBody;

  /// No description provided for @trustChipLabel.
  ///
  /// In en, this message translates to:
  /// **'LOCAL'**
  String get trustChipLabel;

  /// No description provided for @trustChipTooltip.
  ///
  /// In en, this message translates to:
  /// **'Local Only policy · executing on device. Open the Trust Pulse.'**
  String get trustChipTooltip;

  /// No description provided for @commandPaletteTooltip.
  ///
  /// In en, this message translates to:
  /// **'Search commands'**
  String get commandPaletteTooltip;

  /// No description provided for @commandPaletteHint.
  ///
  /// In en, this message translates to:
  /// **'Go to a surface or run an action…'**
  String get commandPaletteHint;

  /// No description provided for @commandPaletteEmpty.
  ///
  /// In en, this message translates to:
  /// **'No matching commands'**
  String get commandPaletteEmpty;

  /// No description provided for @commandGoTo.
  ///
  /// In en, this message translates to:
  /// **'Go to {surface}'**
  String commandGoTo(String surface);

  /// No description provided for @commandSectionSurfaces.
  ///
  /// In en, this message translates to:
  /// **'Surfaces'**
  String get commandSectionSurfaces;

  /// No description provided for @commandSectionActions.
  ///
  /// In en, this message translates to:
  /// **'Actions'**
  String get commandSectionActions;

  /// No description provided for @commandToggleLens.
  ///
  /// In en, this message translates to:
  /// **'Toggle Harbor Lens'**
  String get commandToggleLens;

  /// No description provided for @commandToggleTheme.
  ///
  /// In en, this message translates to:
  /// **'Switch light/dark theme'**
  String get commandToggleTheme;

  /// No description provided for @commandSwitchLanguage.
  ///
  /// In en, this message translates to:
  /// **'Switch language'**
  String get commandSwitchLanguage;

  /// No description provided for @coreDegradedTitle.
  ///
  /// In en, this message translates to:
  /// **'Native core unavailable'**
  String get coreDegradedTitle;

  /// No description provided for @appTagline.
  ///
  /// In en, this message translates to:
  /// **'Your AI. Your models. Your device. Your work.'**
  String get appTagline;

  /// No description provided for @surfaceTitleHome.
  ///
  /// In en, this message translates to:
  /// **'Home'**
  String get surfaceTitleHome;

  /// No description provided for @homeGreetingMorning.
  ///
  /// In en, this message translates to:
  /// **'Good morning'**
  String get homeGreetingMorning;

  /// No description provided for @homeGreetingAfternoon.
  ///
  /// In en, this message translates to:
  /// **'Good afternoon'**
  String get homeGreetingAfternoon;

  /// No description provided for @homeGreetingEvening.
  ///
  /// In en, this message translates to:
  /// **'Good evening'**
  String get homeGreetingEvening;

  /// No description provided for @homeModelHeading.
  ///
  /// In en, this message translates to:
  /// **'Active model'**
  String get homeModelHeading;

  /// No description provided for @homeSectionActive.
  ///
  /// In en, this message translates to:
  /// **'In progress'**
  String get homeSectionActive;

  /// No description provided for @homeSectionRecent.
  ///
  /// In en, this message translates to:
  /// **'Recent runs'**
  String get homeSectionRecent;

  /// No description provided for @homeRecentEmpty.
  ///
  /// In en, this message translates to:
  /// **'Runs you start appear here with their state.'**
  String get homeRecentEmpty;

  /// No description provided for @knowledgeSourcesCount.
  ///
  /// In en, this message translates to:
  /// **'{count, plural, one{1 source} other{{count} sources}}'**
  String knowledgeSourcesCount(int count);

  /// No description provided for @homeKnowledgeNotOpen.
  ///
  /// In en, this message translates to:
  /// **'Index not open'**
  String get homeKnowledgeNotOpen;

  /// No description provided for @homeOpenActivity.
  ///
  /// In en, this message translates to:
  /// **'Open Activity'**
  String get homeOpenActivity;

  /// No description provided for @homeRequestQueued.
  ///
  /// In en, this message translates to:
  /// **'Request recorded as a durable run'**
  String get homeRequestQueued;

  /// No description provided for @homeRequestFailed.
  ///
  /// In en, this message translates to:
  /// **'The request could not be recorded'**
  String get homeRequestFailed;

  /// No description provided for @quickActionsHeading.
  ///
  /// In en, this message translates to:
  /// **'Quick actions'**
  String get quickActionsHeading;

  /// No description provided for @askSubtitle.
  ///
  /// In en, this message translates to:
  /// **'Grounded answers with citations from your Knowledge'**
  String get askSubtitle;

  /// No description provided for @askYou.
  ///
  /// In en, this message translates to:
  /// **'You'**
  String get askYou;

  /// No description provided for @askEvidenceHeading.
  ///
  /// In en, this message translates to:
  /// **'Evidence'**
  String get askEvidenceHeading;

  /// No description provided for @askRetrievalOnlyNote.
  ///
  /// In en, this message translates to:
  /// **'Retrieval only — no chat model was used.'**
  String get askRetrievalOnlyNote;

  /// No description provided for @askCancelledTitle.
  ///
  /// In en, this message translates to:
  /// **'Generation cancelled'**
  String get askCancelledTitle;

  /// No description provided for @askCancelledBody.
  ///
  /// In en, this message translates to:
  /// **'Nothing was recorded for this question.'**
  String get askCancelledBody;

  /// No description provided for @askModelPicker.
  ///
  /// In en, this message translates to:
  /// **'Chat model'**
  String get askModelPicker;

  /// No description provided for @askClearConversation.
  ///
  /// In en, this message translates to:
  /// **'Clear conversation'**
  String get askClearConversation;

  /// No description provided for @askComposerHint.
  ///
  /// In en, this message translates to:
  /// **'Ask about your files…'**
  String get askComposerHint;

  /// No description provided for @askErrorTitle.
  ///
  /// In en, this message translates to:
  /// **'Generation failed'**
  String get askErrorTitle;

  /// No description provided for @askGroundedBadge.
  ///
  /// In en, this message translates to:
  /// **'GROUNDED'**
  String get askGroundedBadge;

  /// No description provided for @askUngroundedBadge.
  ///
  /// In en, this message translates to:
  /// **'NOT GROUNDED'**
  String get askUngroundedBadge;

  /// No description provided for @workSubtitleEmpty.
  ///
  /// In en, this message translates to:
  /// **'Documents, workbooks, decks and PDFs'**
  String get workSubtitleEmpty;

  /// No description provided for @workPreviewOnly.
  ///
  /// In en, this message translates to:
  /// **'Read-only preview'**
  String get workPreviewOnly;

  /// No description provided for @workPreviewOnlyBody.
  ///
  /// In en, this message translates to:
  /// **'Structured edits, diff and safe save are not enabled in this build; the preview comes from the core\'s qualified extraction.'**
  String get workPreviewOnlyBody;

  /// No description provided for @workCloseFile.
  ///
  /// In en, this message translates to:
  /// **'Close file'**
  String get workCloseFile;

  /// No description provided for @workOpening.
  ///
  /// In en, this message translates to:
  /// **'Opening file…'**
  String get workOpening;

  /// No description provided for @workOpenFailedTitle.
  ///
  /// In en, this message translates to:
  /// **'The file could not be previewed'**
  String get workOpenFailedTitle;

  /// No description provided for @workOpenFailedBody.
  ///
  /// In en, this message translates to:
  /// **'Only DOCX, XLSX, PPTX and PDF files are supported, and the file must be readable.'**
  String get workOpenFailedBody;

  /// No description provided for @workCompatibilityTitle.
  ///
  /// In en, this message translates to:
  /// **'Compatibility notice'**
  String get workCompatibilityTitle;

  /// No description provided for @workCompatibilityBody.
  ///
  /// In en, this message translates to:
  /// **'{count, plural, one{1 part is preserved without rendering. Nothing is executed.} other{{count} parts are preserved without rendering. Nothing is executed.}}'**
  String workCompatibilityBody(int count);

  /// No description provided for @workCompatibilityShow.
  ///
  /// In en, this message translates to:
  /// **'Show parts'**
  String get workCompatibilityShow;

  /// No description provided for @workCompatibilityHide.
  ///
  /// In en, this message translates to:
  /// **'Hide parts'**
  String get workCompatibilityHide;

  /// No description provided for @workOutline.
  ///
  /// In en, this message translates to:
  /// **'Outline'**
  String get workOutline;

  /// No description provided for @workOutlineEmpty.
  ///
  /// In en, this message translates to:
  /// **'No headings'**
  String get workOutlineEmpty;

  /// No description provided for @workParagraphs.
  ///
  /// In en, this message translates to:
  /// **'{count, plural, one{1 paragraph} other{{count} paragraphs}}'**
  String workParagraphs(int count);

  /// No description provided for @workPages.
  ///
  /// In en, this message translates to:
  /// **'{count, plural, one{1 page} other{{count} pages}}'**
  String workPages(int count);

  /// No description provided for @workSlides.
  ///
  /// In en, this message translates to:
  /// **'{count, plural, one{1 slide} other{{count} slides}}'**
  String workSlides(int count);

  /// No description provided for @workCells.
  ///
  /// In en, this message translates to:
  /// **'{count, plural, one{1 cell} other{{count} cells}}'**
  String workCells(int count);

  /// No description provided for @workCharts.
  ///
  /// In en, this message translates to:
  /// **'{count, plural, one{1 chart} other{{count} charts}}'**
  String workCharts(int count);

  /// No description provided for @workFormulaBar.
  ///
  /// In en, this message translates to:
  /// **'Formula'**
  String get workFormulaBar;

  /// No description provided for @workValue.
  ///
  /// In en, this message translates to:
  /// **'Value'**
  String get workValue;

  /// No description provided for @workCellUnverified.
  ///
  /// In en, this message translates to:
  /// **'Cached value — not verified by recalculation'**
  String get workCellUnverified;

  /// No description provided for @workSheetNotPreviewed.
  ///
  /// In en, this message translates to:
  /// **'Only the first sheet is previewed in this build'**
  String get workSheetNotPreviewed;

  /// No description provided for @workPage.
  ///
  /// In en, this message translates to:
  /// **'Page {index}'**
  String workPage(int index);

  /// No description provided for @workSlide.
  ///
  /// In en, this message translates to:
  /// **'Slide {index}'**
  String workSlide(int index);

  /// No description provided for @workKindDocument.
  ///
  /// In en, this message translates to:
  /// **'Document'**
  String get workKindDocument;

  /// No description provided for @workKindWorkbook.
  ///
  /// In en, this message translates to:
  /// **'Workbook'**
  String get workKindWorkbook;

  /// No description provided for @workKindDeck.
  ///
  /// In en, this message translates to:
  /// **'Presentation'**
  String get workKindDeck;

  /// No description provided for @workKindPdf.
  ///
  /// In en, this message translates to:
  /// **'PDF'**
  String get workKindPdf;

  /// No description provided for @workSupportedTypes.
  ///
  /// In en, this message translates to:
  /// **'Supported types'**
  String get workSupportedTypes;

  /// No description provided for @workEmptyTextPage.
  ///
  /// In en, this message translates to:
  /// **'No text extracted on this page'**
  String get workEmptyTextPage;

  /// No description provided for @workShowFormulas.
  ///
  /// In en, this message translates to:
  /// **'Show formulas'**
  String get workShowFormulas;

  /// No description provided for @workNoSelection.
  ///
  /// In en, this message translates to:
  /// **'Select a cell'**
  String get workNoSelection;

  /// No description provided for @modelsSubtitle.
  ///
  /// In en, this message translates to:
  /// **'Install, inspect and choose what runs on this device'**
  String get modelsSubtitle;

  /// No description provided for @modelsImportBody.
  ///
  /// In en, this message translates to:
  /// **'Install a local GGUF file. Packages are data only — no repository code ever executes.'**
  String get modelsImportBody;

  /// No description provided for @modelsUseForAsk.
  ///
  /// In en, this message translates to:
  /// **'Use for Ask'**
  String get modelsUseForAsk;

  /// No description provided for @modelsInUse.
  ///
  /// In en, this message translates to:
  /// **'In use'**
  String get modelsInUse;

  /// No description provided for @modelsRuntime.
  ///
  /// In en, this message translates to:
  /// **'Runtime'**
  String get modelsRuntime;

  /// No description provided for @modelsFiles.
  ///
  /// In en, this message translates to:
  /// **'Files'**
  String get modelsFiles;

  /// No description provided for @modelsSize.
  ///
  /// In en, this message translates to:
  /// **'Size'**
  String get modelsSize;

  /// No description provided for @modelsRecommendedBody.
  ///
  /// In en, this message translates to:
  /// **'Recommendations are ranked by Fit Score once the signed catalog is synced. Until then, search Hugging Face or import a local package.'**
  String get modelsRecommendedBody;

  /// No description provided for @modelsHfSearchHint.
  ///
  /// In en, this message translates to:
  /// **'Search public repositories'**
  String get modelsHfSearchHint;

  /// No description provided for @modelsDownloads.
  ///
  /// In en, this message translates to:
  /// **'{count, plural, one{1 download} other{{count} downloads}}'**
  String modelsDownloads(int count);

  /// No description provided for @modelsLikes.
  ///
  /// In en, this message translates to:
  /// **'{count, plural, one{1 like} other{{count} likes}}'**
  String modelsLikes(int count);

  /// No description provided for @modelsAcquireTitle.
  ///
  /// In en, this message translates to:
  /// **'Acquiring model'**
  String get modelsAcquireTitle;

  /// No description provided for @modelsInstalledCount.
  ///
  /// In en, this message translates to:
  /// **'{count, plural, one{1 installed} other{{count} installed}}'**
  String modelsInstalledCount(int count);

  /// No description provided for @modelsGoHuggingFace.
  ///
  /// In en, this message translates to:
  /// **'Search Hugging Face'**
  String get modelsGoHuggingFace;

  /// No description provided for @modelsImportGguf.
  ///
  /// In en, this message translates to:
  /// **'Import GGUF'**
  String get modelsImportGguf;

  /// No description provided for @modelsFitComputing.
  ///
  /// In en, this message translates to:
  /// **'Computing Fit Score…'**
  String get modelsFitComputing;

  /// No description provided for @fitLabelExcellent.
  ///
  /// In en, this message translates to:
  /// **'Excellent'**
  String get fitLabelExcellent;

  /// No description provided for @fitLabelGood.
  ///
  /// In en, this message translates to:
  /// **'Good'**
  String get fitLabelGood;

  /// No description provided for @fitLabelLimited.
  ///
  /// In en, this message translates to:
  /// **'Limited'**
  String get fitLabelLimited;

  /// No description provided for @fitLabelTooLarge.
  ///
  /// In en, this message translates to:
  /// **'Too large'**
  String get fitLabelTooLarge;

  /// No description provided for @fitLabelUnsupported.
  ///
  /// In en, this message translates to:
  /// **'Unsupported'**
  String get fitLabelUnsupported;

  /// No description provided for @fileGroupModels.
  ///
  /// In en, this message translates to:
  /// **'Model packages'**
  String get fileGroupModels;

  /// No description provided for @agentsSubtitle.
  ///
  /// In en, this message translates to:
  /// **'Profiles that combine a model, tools, skills, knowledge and policy'**
  String get agentsSubtitle;

  /// No description provided for @agentsUnavailableTitle.
  ///
  /// In en, this message translates to:
  /// **'Not enabled in this release'**
  String get agentsUnavailableTitle;

  /// No description provided for @agentsWhatTitle.
  ///
  /// In en, this message translates to:
  /// **'What an agent profile will contain'**
  String get agentsWhatTitle;

  /// No description provided for @agentsPartModel.
  ///
  /// In en, this message translates to:
  /// **'Model'**
  String get agentsPartModel;

  /// No description provided for @agentsPartModelBody.
  ///
  /// In en, this message translates to:
  /// **'A qualified installed model with an explicit, sticky lock per workspace.'**
  String get agentsPartModelBody;

  /// No description provided for @agentsPartTools.
  ///
  /// In en, this message translates to:
  /// **'Tools'**
  String get agentsPartTools;

  /// No description provided for @agentsPartToolsBody.
  ///
  /// In en, this message translates to:
  /// **'An allowlist drawn from the built-in skill families.'**
  String get agentsPartToolsBody;

  /// No description provided for @agentsPartKnowledge.
  ///
  /// In en, this message translates to:
  /// **'Knowledge'**
  String get agentsPartKnowledge;

  /// No description provided for @agentsPartKnowledgeBody.
  ///
  /// In en, this message translates to:
  /// **'Collections the agent may cite, never sources it was not granted.'**
  String get agentsPartKnowledgeBody;

  /// No description provided for @agentsPartPolicy.
  ///
  /// In en, this message translates to:
  /// **'Policy & approvals'**
  String get agentsPartPolicy;

  /// No description provided for @agentsPartPolicyBody.
  ///
  /// In en, this message translates to:
  /// **'Protected effects pause for a Harbor Sheet review before anything is written.'**
  String get agentsPartPolicyBody;

  /// No description provided for @agentsGoSkills.
  ///
  /// In en, this message translates to:
  /// **'Browse skills'**
  String get agentsGoSkills;

  /// No description provided for @agentsGoActivity.
  ///
  /// In en, this message translates to:
  /// **'View runs'**
  String get agentsGoActivity;

  /// No description provided for @skillsSubtitle.
  ///
  /// In en, this message translates to:
  /// **'Built-in skill definitions. Graph skills run through the durable executor; prose skills are declarations until decomposed.'**
  String get skillsSubtitle;

  /// No description provided for @skillsSearchHint.
  ///
  /// In en, this message translates to:
  /// **'Filter skills'**
  String get skillsSearchHint;

  /// No description provided for @skillsCount.
  ///
  /// In en, this message translates to:
  /// **'{count, plural, one{1 skill} other{{count} skills}}'**
  String skillsCount(int count);

  /// No description provided for @skillsToolsHeading.
  ///
  /// In en, this message translates to:
  /// **'Tools'**
  String get skillsToolsHeading;

  /// No description provided for @skillsNoMatch.
  ///
  /// In en, this message translates to:
  /// **'No skills match the filter'**
  String get skillsNoMatch;

  /// No description provided for @skillsFamily.
  ///
  /// In en, this message translates to:
  /// **'Family'**
  String get skillsFamily;

  /// No description provided for @skillsBuiltIn.
  ///
  /// In en, this message translates to:
  /// **'Built-in'**
  String get skillsBuiltIn;

  /// No description provided for @knowledgeSubtitle.
  ///
  /// In en, this message translates to:
  /// **'A local, citation-backed index. Sources never leave the device.'**
  String get knowledgeSubtitle;

  /// No description provided for @knowledgeIndexHeading.
  ///
  /// In en, this message translates to:
  /// **'Index'**
  String get knowledgeIndexHeading;

  /// No description provided for @knowledgeIdentity.
  ///
  /// In en, this message translates to:
  /// **'Identity'**
  String get knowledgeIdentity;

  /// No description provided for @knowledgeDimension.
  ///
  /// In en, this message translates to:
  /// **'Dimensions'**
  String get knowledgeDimension;

  /// No description provided for @knowledgeEmbedding.
  ///
  /// In en, this message translates to:
  /// **'Embedding model'**
  String get knowledgeEmbedding;

  /// No description provided for @knowledgeRemoveConfirmTitle.
  ///
  /// In en, this message translates to:
  /// **'Remove {title}?'**
  String knowledgeRemoveConfirmTitle(String title);

  /// No description provided for @knowledgeRemoveConfirmBody.
  ///
  /// In en, this message translates to:
  /// **'Its chunks leave the index. Past citations will show the source as removed.'**
  String get knowledgeRemoveConfirmBody;

  /// No description provided for @knowledgeOpening.
  ///
  /// In en, this message translates to:
  /// **'Opening the index…'**
  String get knowledgeOpening;

  /// No description provided for @knowledgeGoModels.
  ///
  /// In en, this message translates to:
  /// **'Open Models'**
  String get knowledgeGoModels;

  /// No description provided for @knowledgeSourceKb.
  ///
  /// In en, this message translates to:
  /// **'{kb} KB'**
  String knowledgeSourceKb(int kb);

  /// No description provided for @activitySubtitle.
  ///
  /// In en, this message translates to:
  /// **'Durable runs and background operations'**
  String get activitySubtitle;

  /// No description provided for @activityTabRuns.
  ///
  /// In en, this message translates to:
  /// **'Runs'**
  String get activityTabRuns;

  /// No description provided for @activityTabOps.
  ///
  /// In en, this message translates to:
  /// **'Operations'**
  String get activityTabOps;

  /// No description provided for @activityOpsEmptyTitle.
  ///
  /// In en, this message translates to:
  /// **'No background operations'**
  String get activityOpsEmptyTitle;

  /// No description provided for @activityOpsEmptyBody.
  ///
  /// In en, this message translates to:
  /// **'Downloads, indexing and generation appear here while they run and after they finish.'**
  String get activityOpsEmptyBody;

  /// No description provided for @activityRunDetail.
  ///
  /// In en, this message translates to:
  /// **'Run detail'**
  String get activityRunDetail;

  /// No description provided for @activityFinalState.
  ///
  /// In en, this message translates to:
  /// **'Final state'**
  String get activityFinalState;

  /// No description provided for @activityVerifiedEvents.
  ///
  /// In en, this message translates to:
  /// **'Verified events'**
  String get activityVerifiedEvents;

  /// No description provided for @activityTrailHeading.
  ///
  /// In en, this message translates to:
  /// **'Run Trail'**
  String get activityTrailHeading;

  /// No description provided for @activityRunsCount.
  ///
  /// In en, this message translates to:
  /// **'{count, plural, =0{No runs} one{1 run} other{{count} runs}}'**
  String activityRunsCount(int count);

  /// No description provided for @activityReplayFailed.
  ///
  /// In en, this message translates to:
  /// **'The run could not be replayed'**
  String get activityReplayFailed;

  /// No description provided for @activityOpKindAcquire.
  ///
  /// In en, this message translates to:
  /// **'Model acquisition'**
  String get activityOpKindAcquire;

  /// No description provided for @activityOpKindIngest.
  ///
  /// In en, this message translates to:
  /// **'Knowledge indexing'**
  String get activityOpKindIngest;

  /// No description provided for @activityOpKindGenerate.
  ///
  /// In en, this message translates to:
  /// **'Grounded generation'**
  String get activityOpKindGenerate;

  /// No description provided for @settingsSubtitle.
  ///
  /// In en, this message translates to:
  /// **'Appearance, language, privacy and identity'**
  String get settingsSubtitle;

  /// No description provided for @settingsThemeSystem.
  ///
  /// In en, this message translates to:
  /// **'System'**
  String get settingsThemeSystem;

  /// No description provided for @settingsLanguageBody.
  ///
  /// In en, this message translates to:
  /// **'Arabic mirrors navigation and alignment; file names, formulas and identifiers keep their own direction.'**
  String get settingsLanguageBody;

  /// No description provided for @settingsPrivacyBody.
  ///
  /// In en, this message translates to:
  /// **'Local Only is the default and the only policy in this release.'**
  String get settingsPrivacyBody;

  /// No description provided for @settingsAbout.
  ///
  /// In en, this message translates to:
  /// **'About'**
  String get settingsAbout;

  /// No description provided for @settingsVersion.
  ///
  /// In en, this message translates to:
  /// **'Version'**
  String get settingsVersion;

  /// No description provided for @settingsCoreStatus.
  ///
  /// In en, this message translates to:
  /// **'Native core'**
  String get settingsCoreStatus;

  /// No description provided for @settingsCoreLoaded.
  ///
  /// In en, this message translates to:
  /// **'Loaded'**
  String get settingsCoreLoaded;

  /// No description provided for @settingsCoreDegraded.
  ///
  /// In en, this message translates to:
  /// **'Not loaded — degraded'**
  String get settingsCoreDegraded;

  /// No description provided for @settingsShortcuts.
  ///
  /// In en, this message translates to:
  /// **'Keyboard shortcuts'**
  String get settingsShortcuts;

  /// No description provided for @settingsShortcutSurfaces.
  ///
  /// In en, this message translates to:
  /// **'Switch surfaces'**
  String get settingsShortcutSurfaces;

  /// No description provided for @settingsShortcutPalette.
  ///
  /// In en, this message translates to:
  /// **'Command palette'**
  String get settingsShortcutPalette;

  /// No description provided for @settingsLensDocked.
  ///
  /// In en, this message translates to:
  /// **'Dock the Lens on wide windows'**
  String get settingsLensDocked;

  /// No description provided for @settingsMotionNote.
  ///
  /// In en, this message translates to:
  /// **'Motion follows your system\'s reduce-motion setting.'**
  String get settingsMotionNote;

  /// No description provided for @copyAction.
  ///
  /// In en, this message translates to:
  /// **'Copy'**
  String get copyAction;

  /// No description provided for @copiedMessage.
  ///
  /// In en, this message translates to:
  /// **'Copied'**
  String get copiedMessage;

  /// No description provided for @activityExecutorTime.
  ///
  /// In en, this message translates to:
  /// **'Executor time'**
  String get activityExecutorTime;

  /// No description provided for @activityStepsLabel.
  ///
  /// In en, this message translates to:
  /// **'Steps'**
  String get activityStepsLabel;

  /// No description provided for @skillsRunnable.
  ///
  /// In en, this message translates to:
  /// **'Runnable graph'**
  String get skillsRunnable;

  /// No description provided for @skillsDeclaration.
  ///
  /// In en, this message translates to:
  /// **'Declaration only'**
  String get skillsDeclaration;

  /// No description provided for @skillsDeclarationBody.
  ///
  /// In en, this message translates to:
  /// **'This skill is a prose declaration: it has no executable graph yet, so nothing runs it. Its instructions and allowlist are the spec for a future graph.'**
  String get skillsDeclarationBody;

  /// No description provided for @skillsGraphHeading.
  ///
  /// In en, this message translates to:
  /// **'Graph'**
  String get skillsGraphHeading;

  /// No description provided for @skillsGraphNodes.
  ///
  /// In en, this message translates to:
  /// **'{count, plural, one{1 node} other{{count} nodes}}'**
  String skillsGraphNodes(int count);

  /// No description provided for @skillsGraphModelNodes.
  ///
  /// In en, this message translates to:
  /// **'{count, plural, =0{No model calls — fully deterministic} one{1 model node, schema-constrained} other{{count} model nodes, schema-constrained}}'**
  String skillsGraphModelNodes(int count);

  /// No description provided for @skillsGraphBudgets.
  ///
  /// In en, this message translates to:
  /// **'Budget: {steps} steps, {tools} tool calls'**
  String skillsGraphBudgets(int steps, int tools);

  /// No description provided for @skillsRun.
  ///
  /// In en, this message translates to:
  /// **'Run'**
  String get skillsRun;

  /// No description provided for @skillsRunTitle.
  ///
  /// In en, this message translates to:
  /// **'Run {title}'**
  String skillsRunTitle(String title);

  /// No description provided for @skillsAttachFile.
  ///
  /// In en, this message translates to:
  /// **'Attach file'**
  String get skillsAttachFile;

  /// No description provided for @skillsAttached.
  ///
  /// In en, this message translates to:
  /// **'Attached: {name}'**
  String skillsAttached(String name);

  /// No description provided for @skillsValuesHint.
  ///
  /// In en, this message translates to:
  /// **'One value per line: key = value'**
  String get skillsValuesHint;

  /// No description provided for @skillsNeedsModel.
  ///
  /// In en, this message translates to:
  /// **'This skill has model nodes. Install a chat model in Models to run it; the model only fills typed slots.'**
  String get skillsNeedsModel;

  /// No description provided for @skillsModelLabel.
  ///
  /// In en, this message translates to:
  /// **'Model'**
  String get skillsModelLabel;

  /// No description provided for @skillsRunning.
  ///
  /// In en, this message translates to:
  /// **'Running on device…'**
  String get skillsRunning;

  /// No description provided for @skillsRunFailed.
  ///
  /// In en, this message translates to:
  /// **'Run failed'**
  String get skillsRunFailed;

  /// No description provided for @skillsOutcome.
  ///
  /// In en, this message translates to:
  /// **'Outcome'**
  String get skillsOutcome;

  /// No description provided for @skillsOutcomeCompleted.
  ///
  /// In en, this message translates to:
  /// **'Completed'**
  String get skillsOutcomeCompleted;

  /// No description provided for @skillsOutcomeNeedsInput.
  ///
  /// In en, this message translates to:
  /// **'Needs input — nothing was invented'**
  String get skillsOutcomeNeedsInput;

  /// No description provided for @skillsOutcomeAbstained.
  ///
  /// In en, this message translates to:
  /// **'Abstained'**
  String get skillsOutcomeAbstained;

  /// No description provided for @skillsRunState.
  ///
  /// In en, this message translates to:
  /// **'Run state'**
  String get skillsRunState;

  /// No description provided for @skillsTrailHeading.
  ///
  /// In en, this message translates to:
  /// **'Node trail'**
  String get skillsTrailHeading;

  /// No description provided for @skillsApprovalTitle.
  ///
  /// In en, this message translates to:
  /// **'Approval required'**
  String get skillsApprovalTitle;

  /// No description provided for @skillsApprovalBody.
  ///
  /// In en, this message translates to:
  /// **'The run proposed a {effect} affecting {count, plural, one{1 operation} other{{count} operations}}. Nothing has been written. The proposal is bound to the base content hash and the hash of the output it would produce.'**
  String skillsApprovalBody(String effect, int count);

  /// No description provided for @skillsApprove.
  ///
  /// In en, this message translates to:
  /// **'Approve'**
  String get skillsApprove;

  /// No description provided for @skillsReject.
  ///
  /// In en, this message translates to:
  /// **'Reject'**
  String get skillsReject;

  /// No description provided for @skillsBaseHash.
  ///
  /// In en, this message translates to:
  /// **'Base content hash'**
  String get skillsBaseHash;

  /// No description provided for @skillsProposedHash.
  ///
  /// In en, this message translates to:
  /// **'Proposed output hash'**
  String get skillsProposedHash;

  /// No description provided for @skillsOutputsHeading.
  ///
  /// In en, this message translates to:
  /// **'Outputs'**
  String get skillsOutputsHeading;

  /// No description provided for @skillsRequiredField.
  ///
  /// In en, this message translates to:
  /// **'Required'**
  String get skillsRequiredField;

  /// No description provided for @skillsCancelRun.
  ///
  /// In en, this message translates to:
  /// **'Cancel run'**
  String get skillsCancelRun;

  /// No description provided for @skillsInputsHeading.
  ///
  /// In en, this message translates to:
  /// **'Inputs'**
  String get skillsInputsHeading;

  /// No description provided for @skillsExecutedOn.
  ///
  /// In en, this message translates to:
  /// **'Executed on {model}'**
  String skillsExecutedOn(String model);

  /// No description provided for @skillsStructuredMode.
  ///
  /// In en, this message translates to:
  /// **'Structured output: {mode}'**
  String skillsStructuredMode(String mode);

  /// No description provided for @skillsCommitBody.
  ///
  /// In en, this message translates to:
  /// **'The run proposed a {effect} affecting {count, plural, one{1 operation} other{{count} operations}}. Nothing has been written yet. Save new copy writes the approved output as a new file; the original is never modified. The proposal is bound to the base content hash and the hash of the output it produces.'**
  String skillsCommitBody(String effect, int count);

  /// No description provided for @skillsSaveNewCopy.
  ///
  /// In en, this message translates to:
  /// **'Save new copy'**
  String get skillsSaveNewCopy;

  /// No description provided for @skillsOverwrite.
  ///
  /// In en, this message translates to:
  /// **'Overwrite original…'**
  String get skillsOverwrite;

  /// No description provided for @skillsOverwriteConfirmTitle.
  ///
  /// In en, this message translates to:
  /// **'Overwrite the original?'**
  String get skillsOverwriteConfirmTitle;

  /// No description provided for @skillsOverwriteConfirmBody.
  ///
  /// In en, this message translates to:
  /// **'Harbor will replace {name} in place. This only happens if the file still matches the approved base; otherwise nothing is written and the run stops with the reason.'**
  String skillsOverwriteConfirmBody(String name);

  /// No description provided for @skillsSavedNewCopy.
  ///
  /// In en, this message translates to:
  /// **'Saved as a new copy'**
  String get skillsSavedNewCopy;

  /// No description provided for @skillsOverwritten.
  ///
  /// In en, this message translates to:
  /// **'Original replaced'**
  String get skillsOverwritten;

  /// No description provided for @skillsCommittedTo.
  ///
  /// In en, this message translates to:
  /// **'{path} · version {version}'**
  String skillsCommittedTo(String path, String version);

  /// No description provided for @skillsDiffBase.
  ///
  /// In en, this message translates to:
  /// **'base'**
  String get skillsDiffBase;

  /// No description provided for @skillsDiffProposed.
  ///
  /// In en, this message translates to:
  /// **'proposed'**
  String get skillsDiffProposed;

  /// No description provided for @settingsDiagnostics.
  ///
  /// In en, this message translates to:
  /// **'Diagnostics'**
  String get settingsDiagnostics;

  /// No description provided for @settingsDiagnosticsBody.
  ///
  /// In en, this message translates to:
  /// **'Harbor keeps an encrypted crash and error log on this device and never uploads it. Export it to share with support by hand.'**
  String get settingsDiagnosticsBody;

  /// No description provided for @settingsDiagnosticsContains.
  ///
  /// In en, this message translates to:
  /// **'The export contains redacted error records, the app and core versions, the runtime, the device class and installed model ids. It never contains document content, knowledge chunks or prompts.'**
  String get settingsDiagnosticsContains;

  /// No description provided for @settingsDiagnosticsRecords.
  ///
  /// In en, this message translates to:
  /// **'{count, plural, =0{No records yet} one{1 record} other{{count} records}}'**
  String settingsDiagnosticsRecords(int count);

  /// No description provided for @settingsDiagnosticsExport.
  ///
  /// In en, this message translates to:
  /// **'Export diagnostics'**
  String get settingsDiagnosticsExport;

  /// No description provided for @settingsDiagnosticsExported.
  ///
  /// In en, this message translates to:
  /// **'Diagnostics exported'**
  String get settingsDiagnosticsExported;

  /// No description provided for @settingsDiagnosticsExportedBody.
  ///
  /// In en, this message translates to:
  /// **'{path} · {count, plural, one{1 record} other{{count} records}}'**
  String settingsDiagnosticsExportedBody(String path, int count);

  /// No description provided for @settingsDiagnosticsExportFailed.
  ///
  /// In en, this message translates to:
  /// **'Export failed'**
  String get settingsDiagnosticsExportFailed;

  /// No description provided for @homeFirstRunTitle.
  ///
  /// In en, this message translates to:
  /// **'Install a model to get started'**
  String get homeFirstRunTitle;

  /// No description provided for @homeFirstRunBody.
  ///
  /// In en, this message translates to:
  /// **'Harbor runs entirely on this device — nothing you open leaves it. Every answer, skill and index needs a local model, so the first step is choosing one that fits your device.'**
  String get homeFirstRunBody;

  /// No description provided for @homeFirstRunAction.
  ///
  /// In en, this message translates to:
  /// **'Choose a model'**
  String get homeFirstRunAction;

  /// No description provided for @modelsFirstRunTitle.
  ///
  /// In en, this message translates to:
  /// **'Local only'**
  String get modelsFirstRunTitle;

  /// No description provided for @modelsFirstRunBody.
  ///
  /// In en, this message translates to:
  /// **'These packages come from Harbor\'s signed catalog. Check size & fit reads the file list from Hugging Face through the broker; Install downloads once and verifies the pinned hash. Nothing else leaves the device.'**
  String get modelsFirstRunBody;

  /// No description provided for @modelsCatalogHeading.
  ///
  /// In en, this message translates to:
  /// **'{count, plural, one{1 catalog package} other{{count} catalog packages}}'**
  String modelsCatalogHeading(int count);

  /// No description provided for @modelsCatalogFooter.
  ///
  /// In en, this message translates to:
  /// **'Fit Score is computed by the core from this device\'s memory and accelerator; it is never guessed. Sizes come from the repository listing, not the catalog.'**
  String get modelsCatalogFooter;

  /// No description provided for @modelsCatalogCheckFit.
  ///
  /// In en, this message translates to:
  /// **'Check size & fit'**
  String get modelsCatalogCheckFit;

  /// No description provided for @modelsCatalogInstall.
  ///
  /// In en, this message translates to:
  /// **'Install'**
  String get modelsCatalogInstall;

  /// No description provided for @modelsCatalogSizeUnavailable.
  ///
  /// In en, this message translates to:
  /// **'Size unavailable (the repository listing could not be read).'**
  String get modelsCatalogSizeUnavailable;

  /// No description provided for @modelsInstalledBadge.
  ///
  /// In en, this message translates to:
  /// **'Installed'**
  String get modelsInstalledBadge;

  /// No description provided for @modelsContextTokens.
  ///
  /// In en, this message translates to:
  /// **'{count} tokens context'**
  String modelsContextTokens(int count);

  /// No description provided for @opAcquireRunning.
  ///
  /// In en, this message translates to:
  /// **'Installing…'**
  String get opAcquireRunning;
}

class _AppLocalizationsDelegate
    extends LocalizationsDelegate<AppLocalizations> {
  const _AppLocalizationsDelegate();

  @override
  Future<AppLocalizations> load(Locale locale) {
    return SynchronousFuture<AppLocalizations>(lookupAppLocalizations(locale));
  }

  @override
  bool isSupported(Locale locale) =>
      <String>['ar', 'en'].contains(locale.languageCode);

  @override
  bool shouldReload(_AppLocalizationsDelegate old) => false;
}

AppLocalizations lookupAppLocalizations(Locale locale) {
  // Lookup logic when only language code is specified.
  switch (locale.languageCode) {
    case 'ar':
      return AppLocalizationsAr();
    case 'en':
      return AppLocalizationsEn();
  }

  throw FlutterError(
      'AppLocalizations.delegate failed to load unsupported locale "$locale". This is likely '
      'an issue with the localizations generation tool. Please file an issue '
      'on GitHub with a reproducible sample app and the gen-l10n configuration '
      'that was used.');
}

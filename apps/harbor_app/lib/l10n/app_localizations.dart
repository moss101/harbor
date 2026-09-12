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
  /// **'Built-in skills ready'**
  String get skillsEmptyTitle;

  /// No description provided for @skillsEmptyBody.
  ///
  /// In en, this message translates to:
  /// **'Ship-quality skills for documents, spreadsheets, research, translation and more.'**
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
  /// **'{count} tools'**
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

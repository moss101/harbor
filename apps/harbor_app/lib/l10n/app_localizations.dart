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

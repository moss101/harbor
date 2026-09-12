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
}

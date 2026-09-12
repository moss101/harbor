// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for Arabic (`ar`).
class AppLocalizationsAr extends AppLocalizations {
  AppLocalizationsAr([String locale = 'ar']) : super(locale);

  @override
  String get surfaceHome => 'الرئيسية';

  @override
  String get surfaceAsk => 'اسأل';

  @override
  String get surfaceWork => 'العمل';

  @override
  String get surfaceAgents => 'الوكلاء';

  @override
  String get surfaceModels => 'النماذج';

  @override
  String get surfaceSkills => 'المهارات';

  @override
  String get surfaceKnowledge => 'المعرفة';

  @override
  String get surfaceActivity => 'النشاط';

  @override
  String get surfaceSettings => 'الإعدادات';

  @override
  String get homeHeadline => 'ما الذي تريد إنجازه؟';

  @override
  String get homeComposerHint => 'صف العمل، أرفق الملفات، ثم ابدأ.';

  @override
  String get quickSummarize => 'تلخيص مستند';

  @override
  String get quickAnalyze => 'تحليل جدول بيانات';

  @override
  String get quickPresent => 'إنشاء عرض تقديمي';

  @override
  String get quickCompare => 'مقارنة الملفات';

  @override
  String get quickOrganize => 'تنظيم المشروع';

  @override
  String get quickResearch => 'بحث في هذا المجلد';

  @override
  String get quickTranslate => 'ترجمة المحتوى';

  @override
  String get workEmptyTitle => 'لا يوجد ملف مفتوح';

  @override
  String get workEmptyBody =>
      'Open a document, workbook, deck or PDF to see it here with version, verification and conflict state.';

  @override
  String get modelsInstalled => 'المثبتة';

  @override
  String get modelsRecommended => 'الموصى بها';

  @override
  String get modelsLibrary => 'مكتبة هاربر';

  @override
  String get modelsHuggingFace => 'هاجينج فيس';

  @override
  String get modelsImport => 'استيراد';

  @override
  String get modelsBenchmark => 'قياس الأداء';

  @override
  String get fitScore => 'مقياس الملاءمة';

  @override
  String get trustPolicy => 'السياسة: محلي فقط';

  @override
  String get trustExecutionOnDevice => 'التنفيذ: على الجهاز';

  @override
  String get runTrailEmpty => 'لا يوجد نشاط بعد.';

  @override
  String get approvalApprove => 'موافقة';

  @override
  String get approvalDeny => 'رفض';

  @override
  String get settingsLanguage => 'اللغة';

  @override
  String get settingsEnglish => 'English';

  @override
  String get settingsArabic => 'العربية';

  @override
  String get settingsTheme => 'المظهر';

  @override
  String get settingsThemeLight => 'فاتح';

  @override
  String get settingsThemeDark => 'داكن';

  @override
  String get settingsPrivacy => 'خصوصية مساحة العمل';

  @override
  String get agentsEmptyTitle => 'لا يوجد وكلاء';

  @override
  String get agentsEmptyBody =>
      'Agents combine a model, tools, skills, knowledge and policy. Create one to delegate multi-step work with durable, inspectable runs.';

  @override
  String get skillsEmptyTitle => 'المهارات المدمجة جاهزة';

  @override
  String get skillsEmptyBody =>
      'Ship-quality skills for documents, spreadsheets, research, translation and more.';

  @override
  String get knowledgeEmptyTitle => 'المعرفة فارغة';

  @override
  String get knowledgeEmptyBody =>
      'Add folders or documents to build a local, citation-backed index. Sources never leave the device under Local Only.';

  @override
  String get activityEmptyTitle => 'لا توجد مهام بعد';

  @override
  String get activityEmptyBody =>
      'Durable agent runs appear here with their full Run Trail — including pause, approval and recovery states.';

  @override
  String get askEmptyTitle => 'اسأل يعمل على ملفاتك';

  @override
  String get askEmptyBody =>
      'Answers ground in your workspace with citations and abstain when evidence is insufficient.';
}

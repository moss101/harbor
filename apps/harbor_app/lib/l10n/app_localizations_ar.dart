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

  @override
  String get lensButton => 'العدسة';

  @override
  String workCanvasRuleActive(int min) {
    return 'قاعدة عرض اللوحة $min بكسل نشطة';
  }

  @override
  String get canvasViewportEditing => 'تحرير بحجم نافذة العرض';

  @override
  String get openFile => 'افتح ملفاً';

  @override
  String sheetLabel(String name) {
    return 'الورقة: $name';
  }

  @override
  String get attachFilesTooltip => 'أرفق ملفات';

  @override
  String get modelDockEmpty => 'لا يوجد نموذج مثبت — افتح النماذج للتثبيت';

  @override
  String get modelDockCoreUnavailable =>
      'النواة غير متاحة — لم يتم تحميل بيئة التشغيل الأصلية';

  @override
  String get modelsLibraryEmpty => 'حزم مكتبة هاربر المنتقاة تظهر هنا.';

  @override
  String get modelsHfEmpty =>
      'ابحث في المستودعات العامة. حزم النماذج بيانات فقط — لا ينفَّذ أي كود من المستودعات أبداً.';

  @override
  String get coreNotLoadedModels =>
      'النواة الأصلية غير محملة؛ النماذج المثبتة غير متاحة. ابنِ core/harbor_ffi لتمكين هذا العرض.';

  @override
  String get modelsInstalledEmpty =>
      'ثبّت نموذجاً من الموصى بها أو المكتبة. يُظهر مقياس الملاءمة ما يستطيع جهازك تشغيله جيداً.';

  @override
  String get modelsBenchmarkEmpty =>
      'أحمال قياس أداء محلية مضبوطة على الجهاز مع هوية النموذج وبيئة التشغيل والجهاز.';

  @override
  String get modelsRecommendedEmpty =>
      'تظهر التوصيات بعد مزامنة الكتالوج. لا يُوصى بنموذج إلا إذا استطاع جهازك تشغيله جيداً.';

  @override
  String get coreNotLoadedSkills =>
      'النواة الأصلية غير محملة؛ المهارات معرّفة في النواة ولا يمكن سردها.';

  @override
  String get coreNotLoadedActivity =>
      'النواة الأصلية غير محملة؛ المهام الدائمة مخزنة في النواة ولا يمكن سردها.';

  @override
  String get newAgent => 'وكيل جديد';

  @override
  String get addSources => 'أضف مصادر';

  @override
  String toolsCount(int count) {
    return '$count أدوات';
  }

  @override
  String filesSizeRuntime(int files, int mb, String runtime) {
    return '$files ملفات · $mb ميجابايت · بيئة التشغيل $runtime';
  }

  @override
  String runStateLine(String state, int ms) {
    return 'الحالة: $state · $ms ملي ثانية تنفيذ';
  }

  @override
  String scoreLine(String pct, String state) {
    return 'النتيجة $pct% · $state';
  }

  @override
  String get askNoEvidenceTitle => 'لا توجد أدلة كافية';

  @override
  String get askNoEvidenceBody =>
      'لا شيء في الفهرس المحلي يدعم هذا السؤال، لذا أمتنع عن الإجابة بدلاً من التخمين.';

  @override
  String get askKnowledgeNotOpen =>
      'المعرفة غير مفتوحة بعد. ثبّت نموذج تضمين (النماذج ← المثبتة) لتأسيس الإجابات محلياً.';

  @override
  String get askAbstentionHeading => 'لم أجد دعماً لذلك في معرفتك.';
}

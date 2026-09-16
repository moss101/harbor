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
      'افتح مستنداً أو جدول بيانات أو عرضاً أو PDF لرؤيته هنا مع حالة الإصدار والتحقق والتعارض.';

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
      'يجمع الوكلاء نموذجاً وأدوات ومهارات ومعرفة وسياسة. أنشئ واحداً لتفويض عمل متعدد الخطوات بمهام دائمة قابلة للفحص.';

  @override
  String get skillsEmptyTitle => 'المهارات المدمجة جاهزة';

  @override
  String get skillsEmptyBody =>
      'مهارات عالية الجودة للمستندات وجداول البيانات والبحث والترجمة وغيرها.';

  @override
  String get knowledgeEmptyTitle => 'المعرفة فارغة';

  @override
  String get knowledgeEmptyBody =>
      'أضف مجلدات أو مستندات لبناء فهرس محلي مدعوم بالاستشهادات. المصادر لا تترك الجهاز في وضع محلي فقط.';

  @override
  String get activityEmptyTitle => 'لا توجد مهام بعد';

  @override
  String get activityEmptyBody =>
      'تظهر مهام الوكلاء الدائمة هنا مع مسار التشغيل الكامل — بما فيه الإيقاف والموافقة والاسترداد.';

  @override
  String get askEmptyTitle => 'اسأل يعمل على ملفاتك';

  @override
  String get askEmptyBody =>
      'تؤسس الإجابات على مساحة عملك مع استشهادات وتمتنع عندما تكون الأدلة غير كافية.';

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
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count أداة',
      many: '$count أداة',
      few: '$count أدوات',
      two: 'أداتان',
      one: 'أداة واحدة',
      zero: 'لا توجد أدوات',
    );
    return '$_temp0';
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

  @override
  String get sendAction => 'إرسال';

  @override
  String get lensOpenTooltip => 'افتح مفتش العدسة';

  @override
  String get askSearchTooltip => 'ابحث في المعرفة';

  @override
  String get statusInstalled => 'مثبت';

  @override
  String get cancelAction => 'إلغاء';

  @override
  String get askGenerateTooltip => 'توليد الإجابة';

  @override
  String get askAnswerHeading => 'الإجابة';

  @override
  String get askCitationsHeading => 'المصادر المستشهد بها';

  @override
  String askExecutedOnLine(String model, int tokens) {
    return 'نُفِّذت على $model · $tokens رمزاً على الجهاز';
  }

  @override
  String get askGenerating => 'جارٍ التوليد على الجهاز…';

  @override
  String askGenerationProgress(int tokens) {
    return '$tokens رمز مولّد';
  }

  @override
  String get askInsufficientNote =>
      'يفيد النموذج بأن الأدلة غير كافية — هذه الإجابة غير مستندة إلى مصادرك.';

  @override
  String get askNoChatModel =>
      'لا يوجد نموذج محادثة مثبّت. تحتاج الإجابات إلى نموذج محادثة (النماذج ← Hugging Face)؛ وعند فتح المعرفة تبقى الاستشهادات المسترجَعة متاحة.';

  @override
  String get askSearchOnly => 'بحث في المعرفة';

  @override
  String get knowledgeSourcesHeading => 'المصادر المفهرسة';

  @override
  String knowledgeChunksCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count جزء',
      many: '$count جزءاً',
      few: '$count أجزاء',
      two: 'جزآن',
      one: 'جزء واحد',
      zero: 'لا توجد أجزاء',
    );
    return '$_temp0';
  }

  @override
  String get knowledgeRemoveAction => 'إزالة';

  @override
  String knowledgeIngesting(int done, int total) {
    return 'جارٍ فهرسة المقاطع $done/$total';
  }

  @override
  String get knowledgeOpenNeedsModel =>
      'ثبّت نموذج التضمين bge-small-en-v1.5 (تبويب النماذج) لبناء الفهرس المحلي.';

  @override
  String get knowledgeAddFilesTooltip => 'إضافة ملفات إلى الفهرس المحلي';

  @override
  String get knowledgeAddTextAction => 'لصق نص';

  @override
  String get knowledgeAddTextTitle => 'فهرسة نص ملصوق';

  @override
  String get knowledgeAddTextTitleHint => 'العنوان';

  @override
  String get knowledgeAddTextBodyHint => 'الصق النص المراد فهرسته';

  @override
  String get knowledgeAddTextConfirm => 'فهرسة';

  @override
  String knowledgeSourceAdded(String title) {
    return 'تمت فهرسة $title';
  }

  @override
  String knowledgeAttachUnsupported(String kind) {
    return 'لا يمكن فهرسة ملفات $kind في هذا الإصدار';
  }

  @override
  String get knowledgeNoSources => 'لا توجد مصادر مفهرسة بعد.';

  @override
  String knowledgeRemoved(String title) {
    return 'تمت إزالة $title';
  }

  @override
  String get agentsUnavailableBody =>
      'تنسيق الوكلاء غير مُفعّل في هذا الإصدار. عائلات المهارات المدمجة (تبويب المهارات) متاحة اليوم.';

  @override
  String get modelInstallAction => 'تثبيت';

  @override
  String get modelInstalling => 'جارٍ التثبيت…';

  @override
  String get modelInstallFailed => 'فشل التثبيت';

  @override
  String get modelInstallCancelled => 'أُلغي التثبيت';

  @override
  String get modelsHfSearchFailed =>
      'فشل البحث — رفضت الشبكة الاستعلام. أعد المحاولة بعد قليل.';

  @override
  String get importModelTooltip => 'تثبيت ملف GGUF محلي';

  @override
  String importModelInstalled(String package) {
    return 'تم تثبيت $package';
  }

  @override
  String get importModelFailed => 'فشل التثبيت المحلي';

  @override
  String get settingsIdentity => 'هوية الجهاز';

  @override
  String get settingsWorkspaceId => 'مساحة العمل';

  @override
  String get opResolving => 'جارٍ تحليل الحزمة…';

  @override
  String opDownloading(int done, int total) {
    return '$done من $total';
  }

  @override
  String get opVerifying => 'جارٍ التحقق من البصمات…';

  @override
  String get opInstalling => 'جارٍ التثبيت…';

  @override
  String get opLoadingModel => 'جارٍ تحميل النموذج…';

  @override
  String get opGenerating => 'جارٍ التوليد…';

  @override
  String get opIngesting => 'جارٍ الفهرسة…';

  @override
  String opBytesMib(int done, int total) {
    return '$done من $total ميبيبايت';
  }

  @override
  String get appStarting => 'جارٍ تشغيل النواة المحلية…';

  @override
  String get coreStartFailed =>
      'تعذّر تحميل النواة الأصلية. يعمل Harbor بشكل محدود بدونها.';

  @override
  String get knowledgeIngestFailed => 'فشلت الفهرسة';

  @override
  String get fileGroupDocuments => 'المستندات';

  @override
  String get navMore => 'المزيد';

  @override
  String get navMoreTitle => 'كل الأقسام';

  @override
  String get lensTitle => 'عدسة هاربر';

  @override
  String get lensSubtitle => 'السياق والمهام والمعرفة';

  @override
  String get lensToggleTooltip => 'إظهار أو إخفاء عدسة هاربر';

  @override
  String get closeAction => 'إغلاق';

  @override
  String get lensSectionTrust => 'نبض الثقة';

  @override
  String get lensSectionOps => 'العمل في الخلفية';

  @override
  String get lensSectionRuns => 'آخر المهام';

  @override
  String get lensSectionModel => 'النموذج النشط';

  @override
  String get lensSectionKnowledge => 'فهرس المعرفة';

  @override
  String get viewAllAction => 'عرض الكل';

  @override
  String get trustPolicyHeading => 'سياسة مساحة العمل';

  @override
  String get trustExecutionHeading => 'التنفيذ الحالي';

  @override
  String trustPolicyVersion(String version) {
    return 'إصدار السياسة $version';
  }

  @override
  String get trustLocalOnlyBody =>
      'الطلبات والملفات لا تغادر هذا الجهاز أبداً. الحصول على النماذج هو الجلسة الوحيدة الصريحة عبر الإنترنت، وتمر عبر وسيط الخروج.';

  @override
  String get trustChipLabel => 'محلي';

  @override
  String get trustChipTooltip =>
      'سياسة محلي فقط · التنفيذ على الجهاز. افتح نبض الثقة.';

  @override
  String get commandPaletteTooltip => 'البحث في الأوامر';

  @override
  String get commandPaletteHint => 'انتقل إلى قسم أو نفّذ إجراءً…';

  @override
  String get commandPaletteEmpty => 'لا توجد أوامر مطابقة';

  @override
  String commandGoTo(String surface) {
    return 'الانتقال إلى $surface';
  }

  @override
  String get commandSectionSurfaces => 'الأقسام';

  @override
  String get commandSectionActions => 'الإجراءات';

  @override
  String get commandToggleLens => 'تبديل عدسة هاربر';

  @override
  String get commandToggleTheme => 'تبديل المظهر الفاتح/الداكن';

  @override
  String get commandSwitchLanguage => 'تبديل اللغة';

  @override
  String get coreDegradedTitle => 'النواة الأصلية غير متاحة';

  @override
  String get appTagline => 'ذكاؤك الاصطناعي. نماذجك. جهازك. عملك.';

  @override
  String get surfaceTitleHome => 'الرئيسية';

  @override
  String get homeGreetingMorning => 'صباح الخير';

  @override
  String get homeGreetingAfternoon => 'طاب يومك';

  @override
  String get homeGreetingEvening => 'مساء الخير';

  @override
  String get homeModelHeading => 'النموذج النشط';

  @override
  String get homeSectionActive => 'قيد التنفيذ';

  @override
  String get homeSectionRecent => 'آخر المهام';

  @override
  String get homeRecentEmpty => 'المهام التي تبدأها تظهر هنا مع حالتها.';

  @override
  String knowledgeSourcesCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count مصدر',
      many: '$count مصدراً',
      few: '$count مصادر',
      two: 'مصدران',
      one: 'مصدر واحد',
      zero: 'لا توجد مصادر',
    );
    return '$_temp0';
  }

  @override
  String get homeKnowledgeNotOpen => 'الفهرس غير مفتوح';

  @override
  String get homeOpenActivity => 'افتح النشاط';

  @override
  String get homeRequestQueued => 'تم تسجيل الطلب كمهمة دائمة';

  @override
  String get homeRequestFailed => 'تعذر تسجيل الطلب';

  @override
  String get quickActionsHeading => 'إجراءات سريعة';

  @override
  String get askSubtitle => 'إجابات مستندة إلى اقتباسات من معرفتك';

  @override
  String get askYou => 'أنت';

  @override
  String get askEvidenceHeading => 'الأدلة';

  @override
  String get askRetrievalOnlyNote => 'استرجاع فقط — لم يُستخدم نموذج محادثة.';

  @override
  String get askCancelledTitle => 'تم إلغاء التوليد';

  @override
  String get askCancelledBody => 'لم يُسجَّل شيء لهذا السؤال.';

  @override
  String get askModelPicker => 'نموذج المحادثة';

  @override
  String get askClearConversation => 'مسح المحادثة';

  @override
  String get askComposerHint => 'اسأل عن ملفاتك…';

  @override
  String get askErrorTitle => 'فشل التوليد';

  @override
  String get askGroundedBadge => 'مستند إلى أدلة';

  @override
  String get askUngroundedBadge => 'غير مستند إلى أدلة';

  @override
  String get workSubtitleEmpty => 'المستندات والمصنفات والعروض وملفات PDF';

  @override
  String get workPreviewOnly => 'معاينة للقراءة فقط';

  @override
  String get workPreviewOnlyBody =>
      'التحرير المنظم والمقارنة والحفظ الآمن غير مفعّلة في هذا الإصدار؛ المعاينة مصدرها الاستخراج المؤهل في النواة.';

  @override
  String get workCloseFile => 'إغلاق الملف';

  @override
  String get workOpening => 'جارٍ فتح الملف…';

  @override
  String get workOpenFailedTitle => 'تعذر معاينة الملف';

  @override
  String get workOpenFailedBody =>
      'يدعم هذا الإصدار ملفات DOCX وXLSX وPPTX وPDF فقط، ويجب أن يكون الملف قابلاً للقراءة.';

  @override
  String get workCompatibilityTitle => 'تنبيه التوافق';

  @override
  String workCompatibilityBody(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: 'يُحتفظ بـ $count جزء دون عرضها. لا يُنفَّذ أي شيء.',
      many: 'يُحتفظ بـ $count جزءاً دون عرضها. لا يُنفَّذ أي شيء.',
      few: 'يُحتفظ بـ $count أجزاء دون عرضها. لا يُنفَّذ أي شيء.',
      two: 'يُحتفظ بجزأين دون عرضهما. لا يُنفَّذ أي شيء.',
      one: 'يُحتفظ بجزء واحد دون عرضه. لا يُنفَّذ أي شيء.',
    );
    return '$_temp0';
  }

  @override
  String get workCompatibilityShow => 'عرض الأجزاء';

  @override
  String get workCompatibilityHide => 'إخفاء الأجزاء';

  @override
  String get workOutline => 'المخطط';

  @override
  String get workOutlineEmpty => 'لا توجد عناوين';

  @override
  String workParagraphs(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count فقرة',
      many: '$count فقرة',
      few: '$count فقرات',
      two: 'فقرتان',
      one: 'فقرة واحدة',
      zero: 'لا توجد فقرات',
    );
    return '$_temp0';
  }

  @override
  String workPages(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count صفحة',
      many: '$count صفحة',
      few: '$count صفحات',
      two: 'صفحتان',
      one: 'صفحة واحدة',
      zero: 'لا توجد صفحات',
    );
    return '$_temp0';
  }

  @override
  String workSlides(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count شريحة',
      many: '$count شريحة',
      few: '$count شرائح',
      two: 'شريحتان',
      one: 'شريحة واحدة',
      zero: 'لا توجد شرائح',
    );
    return '$_temp0';
  }

  @override
  String workCells(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count خلية',
      many: '$count خلية',
      few: '$count خلايا',
      two: 'خليتان',
      one: 'خلية واحدة',
      zero: 'لا توجد خلايا',
    );
    return '$_temp0';
  }

  @override
  String workCharts(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count مخطط',
      many: '$count مخططاً',
      few: '$count مخططات',
      two: 'مخططان',
      one: 'مخطط واحد',
      zero: 'لا توجد مخططات',
    );
    return '$_temp0';
  }

  @override
  String get workFormulaBar => 'الصيغة';

  @override
  String get workValue => 'القيمة';

  @override
  String get workCellUnverified =>
      'قيمة مخزنة — لم يتم التحقق منها بإعادة الحساب';

  @override
  String get workSheetNotPreviewed => 'تُعاين الورقة الأولى فقط في هذا الإصدار';

  @override
  String workPage(int index) {
    return 'صفحة $index';
  }

  @override
  String workSlide(int index) {
    return 'شريحة $index';
  }

  @override
  String get workKindDocument => 'مستند';

  @override
  String get workKindWorkbook => 'مصنف';

  @override
  String get workKindDeck => 'عرض تقديمي';

  @override
  String get workKindPdf => 'PDF';

  @override
  String get workSupportedTypes => 'الأنواع المدعومة';

  @override
  String get workEmptyTextPage => 'لم يُستخرج نص من هذه الصفحة';

  @override
  String get workShowFormulas => 'عرض الصيغ';

  @override
  String get workNoSelection => 'اختر خلية';

  @override
  String get modelsSubtitle => 'ثبّت وافحص واختر ما يعمل على هذا الجهاز';

  @override
  String get modelsImportBody =>
      'ثبّت ملف GGUF محلياً. الحزم بيانات فقط — لا يُنفَّذ أي كود من المستودعات أبداً.';

  @override
  String get modelsUseForAsk => 'استخدم في اسأل';

  @override
  String get modelsInUse => 'قيد الاستخدام';

  @override
  String get modelsRuntime => 'بيئة التشغيل';

  @override
  String get modelsFiles => 'الملفات';

  @override
  String get modelsSize => 'الحجم';

  @override
  String get modelsRecommendedBody =>
      'تُرتَّب التوصيات وفق مقياس الملاءمة بعد مزامنة الكتالوج الموقّع. حتى ذلك الحين، ابحث في هاجينج فيس أو استورد حزمة محلية.';

  @override
  String get modelsHfSearchHint => 'ابحث في المستودعات العامة';

  @override
  String modelsDownloads(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count تنزيل',
      many: '$count تنزيلاً',
      few: '$count تنزيلات',
      two: 'تنزيلان',
      one: 'تنزيل واحد',
      zero: 'لا توجد تنزيلات',
    );
    return '$_temp0';
  }

  @override
  String modelsLikes(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count إعجاب',
      many: '$count إعجاباً',
      few: '$count إعجابات',
      two: 'إعجابان',
      one: 'إعجاب واحد',
      zero: 'لا توجد إعجابات',
    );
    return '$_temp0';
  }

  @override
  String get modelsAcquireTitle => 'جارٍ الحصول على النموذج';

  @override
  String modelsInstalledCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count نموذج مثبت',
      many: '$count نموذجاً مثبتاً',
      few: '$count نماذج مثبتة',
      two: 'نموذجان مثبتان',
      one: 'نموذج واحد مثبت',
      zero: 'لا توجد نماذج مثبتة',
    );
    return '$_temp0';
  }

  @override
  String get modelsGoHuggingFace => 'ابحث في هاجينج فيس';

  @override
  String get modelsImportGguf => 'استيراد GGUF';

  @override
  String get modelsFitComputing => 'جارٍ حساب مقياس الملاءمة…';

  @override
  String get fitLabelExcellent => 'ممتاز';

  @override
  String get fitLabelGood => 'جيد';

  @override
  String get fitLabelLimited => 'محدود';

  @override
  String get fitLabelTooLarge => 'كبير جداً';

  @override
  String get fitLabelUnsupported => 'غير مدعوم';

  @override
  String get fileGroupModels => 'حزم النماذج';

  @override
  String get agentsSubtitle =>
      'ملفات تجمع بين نموذج وأدوات ومهارات ومعرفة وسياسة';

  @override
  String get agentsUnavailableTitle => 'غير مفعّل في هذا الإصدار';

  @override
  String get agentsWhatTitle => 'ما سيحتويه ملف الوكيل';

  @override
  String get agentsPartModel => 'النموذج';

  @override
  String get agentsPartModelBody =>
      'نموذج مثبت ومؤهل مع قفل صريح وثابت لكل مساحة عمل.';

  @override
  String get agentsPartTools => 'الأدوات';

  @override
  String get agentsPartToolsBody =>
      'قائمة مسموح بها مستمدة من عائلات المهارات المدمجة.';

  @override
  String get agentsPartKnowledge => 'المعرفة';

  @override
  String get agentsPartKnowledgeBody =>
      'المجموعات التي يمكن للوكيل الاستشهاد بها، ولا مصادر لم يُمنح إياها.';

  @override
  String get agentsPartPolicy => 'السياسة والموافقات';

  @override
  String get agentsPartPolicyBody =>
      'تتوقف التأثيرات المحمية لمراجعة في ورقة هاربر قبل كتابة أي شيء.';

  @override
  String get agentsGoSkills => 'تصفح المهارات';

  @override
  String get agentsGoActivity => 'عرض المهام';

  @override
  String get skillsSubtitle => 'عائلات مهارات مدمجة وموقّعة متاحة لكل مهمة';

  @override
  String get skillsSearchHint => 'تصفية المهارات';

  @override
  String skillsCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count مهارة',
      many: '$count مهارة',
      few: '$count مهارات',
      two: 'مهارتان',
      one: 'مهارة واحدة',
      zero: 'لا توجد مهارات',
    );
    return '$_temp0';
  }

  @override
  String get skillsToolsHeading => 'الأدوات';

  @override
  String get skillsNoMatch => 'لا توجد مهارات مطابقة';

  @override
  String get skillsFamily => 'العائلة';

  @override
  String get skillsBuiltIn => 'مدمجة';

  @override
  String get knowledgeSubtitle =>
      'فهرس محلي مدعوم بالاقتباسات. المصادر لا تغادر الجهاز أبداً.';

  @override
  String get knowledgeIndexHeading => 'الفهرس';

  @override
  String get knowledgeIdentity => 'الهوية';

  @override
  String get knowledgeDimension => 'الأبعاد';

  @override
  String get knowledgeEmbedding => 'نموذج التضمين';

  @override
  String knowledgeRemoveConfirmTitle(String title) {
    return 'إزالة $title؟';
  }

  @override
  String get knowledgeRemoveConfirmBody =>
      'ستُزال أجزاؤه من الفهرس. ستعرض الاقتباسات السابقة المصدر كمُزال.';

  @override
  String get knowledgeOpening => 'جارٍ فتح الفهرس…';

  @override
  String get knowledgeGoModels => 'افتح النماذج';

  @override
  String knowledgeSourceKb(int kb) {
    return '$kb كيلوبايت';
  }

  @override
  String get activitySubtitle => 'المهام الدائمة والعمليات في الخلفية';

  @override
  String get activityTabRuns => 'المهام';

  @override
  String get activityTabOps => 'العمليات';

  @override
  String get activityOpsEmptyTitle => 'لا توجد عمليات في الخلفية';

  @override
  String get activityOpsEmptyBody =>
      'تظهر التنزيلات والفهرسة والتوليد هنا أثناء تشغيلها وبعد انتهائها.';

  @override
  String get activityRunDetail => 'تفاصيل المهمة';

  @override
  String get activityFinalState => 'الحالة النهائية';

  @override
  String get activityVerifiedEvents => 'الأحداث المتحقق منها';

  @override
  String get activityTrailHeading => 'مسار المهمة';

  @override
  String activityRunsCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count مهمة',
      many: '$count مهمة',
      few: '$count مهام',
      two: 'مهمتان',
      one: 'مهمة واحدة',
      zero: 'لا توجد مهام',
    );
    return '$_temp0';
  }

  @override
  String get activityReplayFailed => 'تعذر إعادة تشغيل المهمة';

  @override
  String get activityOpKindAcquire => 'الحصول على نموذج';

  @override
  String get activityOpKindIngest => 'فهرسة المعرفة';

  @override
  String get activityOpKindGenerate => 'توليد مستند إلى أدلة';

  @override
  String get settingsSubtitle => 'المظهر واللغة والخصوصية والهوية';

  @override
  String get settingsThemeSystem => 'النظام';

  @override
  String get settingsLanguageBody =>
      'تعكس العربية التنقل والمحاذاة؛ وتحتفظ أسماء الملفات والصيغ والمعرّفات باتجاهها الخاص.';

  @override
  String get settingsPrivacyBody =>
      'محلي فقط هو الافتراضي والسياسة الوحيدة في هذا الإصدار.';

  @override
  String get settingsAbout => 'حول';

  @override
  String get settingsVersion => 'الإصدار';

  @override
  String get settingsCoreStatus => 'النواة الأصلية';

  @override
  String get settingsCoreLoaded => 'محمّلة';

  @override
  String get settingsCoreDegraded => 'غير محمّلة — وضع متدهور';

  @override
  String get settingsShortcuts => 'اختصارات لوحة المفاتيح';

  @override
  String get settingsShortcutSurfaces => 'تبديل الأقسام';

  @override
  String get settingsShortcutPalette => 'لوحة الأوامر';

  @override
  String get settingsLensDocked => 'تثبيت العدسة في النوافذ الواسعة';

  @override
  String get settingsMotionNote => 'تتبع الحركة إعداد تقليل الحركة في نظامك.';

  @override
  String get copyAction => 'نسخ';

  @override
  String get copiedMessage => 'تم النسخ';

  @override
  String get activityExecutorTime => 'زمن التنفيذ';

  @override
  String get activityStepsLabel => 'الخطوات';
}

# Harbor — Release Notes (1.0.0 release candidate 1)

## English

First public release candidate of Harbor, the local-first AI workspace.

- **On-device GGUF inference** (llama.cpp, Metal on Apple silicon) with
  greedy-deterministic generation and cooperative cancellation.
- **Signed model catalog** with per-file hash pinning, epoch protection,
  and Fit Score device-fit guidance; acquisition from Hugging Face through
  a brokered, audited, per-hop-authorized egress path.
- **Office artifacts:** DOCX, XLSX (qualified formula engine, safe
  recalculation), PPTX, and PDF extraction — with a compatibility
  classifier that preserves unsupported content as-is or refuses
  explicitly (never silently rewrites).
- **Durable agent runs** with crash-window recovery and replay, user
  authority on every effect, and stale-write protection.
- **Encrypted private storage** verified by byte-level at-rest inspection.
- **Independent privacy qualification:** offline runtime, authorized HF
  acquisition, disabled sync — verified by external traffic capture
  reconciled 1:1 against the in-app Network Log.
- **English & Arabic** UI with full RTL, VoiceOver/TalkBack-class labels,
  200% text scale, keyboard traversal, and contrast-audited theming.
- **Optional features (sync, remote inference, connectors, diagnostics)
  are disabled** in this release and ship with no active code path.

Known limitations in this RC: Windows and minimum-device performance
qualification and store-distribution signing are pending external
resources (Apple/Google credentials, physical devices, Windows hardware).

## العربية

أول إصدار تجريبي عام من هاربر، مساحة العمل الذكاء الاصطناعي المحلية
أولاً.

- **استدلال GGUF على الجهاز** (llama.cpp مع تسريع Metal على معالجات آبل)
  بتوليد حتمي وإمكانية إلغاء تعاونية.
- **فهرس نماذج موقَّع** بتثبيت تجزئة لكل ملف وحماية حقب ومؤشر ملاءمة
  للجهاز؛ والاقتناء من Hugging Face عبر مسار وسيط مُدقَّق ومُخوَّل لكل
  قفزة.
- **مستندات أوفيس:** DOCX وXLSX (محرّك صيغ مؤهَّل وإعادة حساب آمنة)
  وPPTX واستخراج PDF — مع مصنِّف توافق يحفظ المحتوى غير المدعوم كما هو أو
  يرفضه صراحةً (ولا يعيد كتابته بصمت أبداً).
- **تشغيل وكلاء دائم** مع استرجاع بعد الانهيار وإعادة العرض وسلطة
  المستخدم على كل أثر وحماية من الكتابات القديمة.
- **تخزين خاص مشفَّر** مُتحقَّق منه بفحص بايتي كامل.
- **مؤهلات خصوصية مستقلة:** التشغيل دون اتصال، واقتناء HF المُخوَّل،
  وتعطيل المزامنة — بالتحقق عبر التقاط خارجي للشبكة يقارن سجلاً بسجل مع
  «سجل الشبكة» داخل التطبيق.
- **واجهة عربية وإنجليزية** مع دعم RTL كامل وقارئات الشاشة وتكبير النص
  200% والتنقل بلوحة المفاتيح وسمعات مُدقَّقة التباين.
- **الميزات الاختيارية (المزامنة، الاستدلال البعيد، الموصلات، التشخيص)
  معطَّلة** في هذه الإصدارة ولا تتضمن مساراً تنفيذياً نشطاً.

حدود معروفة في هذه النسخة التجريبية: تأهيل Windows وأجهزة الحد الأدنى
وتوقيع النشر متبقٍّ على موارد خارجية (اعتمادات آبل/غوغل وأجهزة فعلية).

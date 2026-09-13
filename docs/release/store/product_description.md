# Harbor — Product Description (release candidate 1.0.0)

## English

**Harbor — your AI workspace, on your device.**

Harbor is a local-first AI workspace for documents, spreadsheets, and
presentations. It runs a real language model on your device — no account,
no telemetry, and your chat content never leaves the machine unless you
explicitly authorize a download.

- **On-device intelligence.** Harbor runs GGUF language models directly on
  your hardware (Metal-accelerated on Apple silicon). Chat, document Q&A,
  and grounded retrieval work without a network connection.
- **Office artifacts with integrity.** Create and edit DOCX documents,
  XLSX workbooks (with a qualified formula engine and safe recalculation),
  and PPTX presentations. Unsupported content is preserved as-is or
  explicitly refused — never silently rewritten.
- **Durable work.** Every agent run is journaled and can be replayed after
  a crash or restart. Nothing you approved is re-executed without your
  authority.
- **Private by architecture.** Workspace content is stored encrypted at
  rest. Harbor keeps a complete, tamper-evident audit log of every network
  request it makes — and by default it makes none.
- **Explicit egress only.** Downloading a model from Hugging Face is an
  explicit, per-request authorization with hash verification against a
  signed catalog. You can watch every connection in the Network Log.
- **English and Arabic.** Full right-to-left layout, Arabic typography,
  and screen-reader support in both languages.

Model availability depends on your device's memory and storage; Harbor
scores each model package (Fit Score) and tells you what fits before you
download it.

## العربية

**هاربر — مساحة عمل الذكاء الاصطناعي على جهازك.**

هاربر مساحة عمل ذكاء اصطناعي محلية أولاً للمستندات وجداول البيانات
والعروض التقديمية. يشغّل نموذج لغة حقيقياً على جهازك — بلا حساب، بلا
تتبع، ولا يغادر محتوى محادثاتك الجهاز إلا إذا أذنتَ صراحةً بتنزيل.

- **ذكاء على الجهاز.** يشغّل هاربر نماذج لغة بصيغة GGUF مباشرةً على عدةتك
  (بتسريع Metal على معالجات آبل). الدردشة وأسئلة المستندات والاسترجاع
  المُسنَد تعمل دون اتصال بالشبكة.
- **مستندات أوفيس بسلامة مضمونة.** أنشئ وحرّر مستندات DOCX ومصنفات XLSX
  (بمحرّك صيغ مؤهَّل وإعادة حساب آمنة) وعروضاً بتنسيق PPTX. المحتوى
  غير المدعوم يُحفظ كما هو أو يُرفض صراحةً — ولا يُعاد كتابته بصمت
  أبداً.
- **عمل دائم.** كل تشغيل لوكيل هاربر مُسجَّل بخطوات قابلة لإعادة العرض
  بعد أي انهيار أو إعادة تشغيل، ولا يُعاد تنفيذ أي أمر اعتمدتَه سابقاً
  دون سلطتك.
- **خصوصية بالتصميم.** محتوى مساحة العمل مخزَّن مشفَّراً على القرص،
  ويحتفظ هاربر بسجل تدقيق كامل مقاوم للتلاعب لكل طلب شبكي — وهو افتراضياً
  لا يُرسل شيئاً.
- **خروج صريح من الشبكة فقط.** تنزيل نموذج من Hugging Face عملية صريحة
  يُذنَب بها لكل طلب، مع تحقق تجزئة مقابل فهرس موقَّع، ويمكنك مراقبة كل
  اتصال في سجل الشبكة.
- **العربية والإنجليزية.** تخطيط كامل من اليمين إلى اليسار، وخطوط
  عربية سليمة، ودعم قارئ الشاشة باللغتين.

يعتمد توفر النماذج على ذاكرة جهازك ومساحته؛ يقيّم هاربر كل حزمة نموذج
(مؤشر الملاءمة Fit Score) ويخبرك بما يتناسب مع جهازك قبل التنزيل.

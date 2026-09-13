# Harbor — Privacy Statement (release candidate 1.0.0)

## English

**The short version.** Harbor processes your content on your device. It has
no account system, sends no telemetry, contains no advertising or tracking,
and by default performs no network activity at all. The only network
feature of the release candidate is downloading model packages you
explicitly request from Hugging Face — each request is authorized by you,
verified by hash, and recorded in an audit log you can inspect inside the
app.

**What is stored on your device.** Your workspaces, chats, agent run
journals, artifact documents, and embeddings stay in the app's private
storage on your device. Private workspace content is encrypted at rest
authenticated encryption keyed to your device's workspace key. Diagnostic
databases (agent journal, artifact store, network audit) live inside the
same private container.

**What leaves your device.** Nothing by default. In "Local Only" mode —
the default and the mode all release qualification ran under — Harbor makes
no network requests at all. When *you* choose to acquire a model package
from Hugging Face, Harbor contacts `huggingface.co` and its CDN for exactly
the metadata and files of the package you chose; credentials are never
attached to downloads and every hop is re-authorized and logged. Optional
features that could use the network (sync, remote inference, connectors,
diagnostics) are **disabled** in this release and contain no active code
path until a future qualification makes them available.

**Independent verification.** Harbor's claims are not self-attested: the
release evidence includes an independent capture of all traffic during
qualification runs, compared 1:1 against Harbor's own audit log, plus a
byte-level inspection of on-device storage for plaintext leaks. Both are
repeated on release builds.

**Children.** Harbor is a productivity tool not directed at children; no
age-restricted content or data collection exists by design (nothing is
collected).

**Changes.** If a future release introduces any data collection or
additional network behavior, this statement will be revised and the store
data-safety declarations updated *before* that release ships.

## العربية

**الخلاصة.** يعالج هاربر محتواك على جهازك. لا حسابات، لا بيانات تتبع، لا
إعلانات، وافتراضياً لا ينشط الشبكة إطلاقاً. الميزة الشبكية الوحيدة في هذه
الإصدارة التجريبية هي تنزيل حزم النماذج التي تطلبها صراحةً من Hugging
Face — كل طلب يحتاج إذنك، ويُتحقق منه بالتجزئة، ويُسجَّل في سجل تدقيق
يمكنك مراجعته داخل التطبيق.

**ما يُخزَّن على جهازك.** مساحات عملك ومحادثاتك وسجلات تشغيل الوكلاء
ومستنداتك وملفات الفهرسة تبقى في تخزين التطبيق الخاص على جهازك. محتوى
مساحة العمل الخاصة مخزَّن مشفَّراً (تشفير مُصادَق مرتبط بمفتاح مساحة عمل
جهازك).

**ما يغادر جهازك.** لا شيء افتراضياً. في وضع «محلي فقط» — وهو الوضع
الافتراضي الذي جرت تحت كامل مؤهلات الإصدار — لا يُصدر هاربر أي طلب شبكي.
حين تختار *أنت* الحصول على حزمة نموذج من Hugging Face، يتصل هاربر
بـ`huggingface.co` وشبكة توزيعها لجلب بيانات وملفات تلك الحزمة فقط، دون
إرفاق أي بيانات اعتماد بالتنزيل، مع إعادة تخويل كل قفزة وتسجيلها. أما
الميزات الاختيارية التي قد تستخدم الشبكة (المزامنة والاستدلال البعيد
والموصلات والتشخيص) فهي **معطَّلة** في هذه الإصدارة ولا يوجد لها مسار
تنفيذي نشط.

**تحقق مستقل.** ادعاءات هاربر ليست إقرارات ذاتية: تتضمن أدلة الإصدار
التقاطاً مستقلاً لكامل حركة الشبكة أثناء التأهيل، مقارَناً واحداً لواحد
مع سجل تدقيق هاربر، إضافة إلى فحص بايت-بايت لتخزين الجهاز بحثاً عن أي
تسريب نصي. وكلاهما يُعاد على إصدارات النشر.

**الإعلانات المقبلة.** إذا أضافت إصدارة قادمة أي جمع بيانات أو سلوك شبكي
إضافي، سيُحدَّث هذا البيان وإعلانات المتجر قبل نشرها.

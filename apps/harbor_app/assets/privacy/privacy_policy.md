# Harbor LM Privacy Policy

Effective date: 3 November 2026. Applies to Harbor LM 1.1 for iPhone, iPad and Android.

Harbor LM is published by Cogniwiss Labs ("we"). This policy explains what happens to your information when you use the app. The same text is shown in the app under Settings → Privacy Policy and at https://harbor-lm.pages.dev/privacy.

## Summary

- You sign in to Harbor LM with Apple or Google. Your account holds your sign-in ID, email address and name. It never holds your conversations, files or recordings.
- The AI runs on your device. Your conversations, prompts, documents, photos, recordings and the answers the model writes are processed on your device and are never sent to us or to any AI service, with one exception you control: if you report an answer, the report you confirm is sent to us (section 4).
- Harbor LM has no advertising, no analytics, no tracking and no crash reporting service.
- Apart from signing in and reporting, the app connects to the internet only when you use the Models screen to search for, check or download an AI model from Hugging Face. Those requests contain the model name or the search words you typed, never your conversations or files.

## 1. Information we collect

Account information. Harbor LM requires an account. When you sign in with Apple or Google, our sign-in service (Firebase Authentication, operated by Google) receives and stores your account's unique ID, the email address the provider gives us (with Apple you can choose to hide it behind a relay address), your name if the provider shares it, the provider you used, and when the account was created and last signed in. Google may also record your IP address when you sign in, for security. We use this information only to let you into the app and to keep the service secure.

Reports. If you report an AI answer, we receive what you confirm in the Report sheet: the answer, the reason you chose, your comment if you write one, your question if you leave "Include my question" on, the model's name, the app version and your platform, linked to your account ID.

Nothing else. We receive nothing else about what you do in the app. Your conversations, files, recordings and Knowledge index stay on your device; we cannot see, recover or delete them.

## 2. What happens on your device

All of the following run entirely on your device:

- Running the AI model: loading the model, writing answers, keeping conversation history and using earlier turns as context.
- Reading and answering questions about your documents, spreadsheets, presentations, PDFs and text files, and creating or editing documents.
- Searching your own files and notes (the "Knowledge" index), including image, audio and video search, which uses a local model.
- Voice input. Speech is turned into text by the on-device speech recognizer of your phone: Apple's on-device SpeechAnalyzer on iOS 26 or later, and Android's on-device speech recognizer on Android 13 or later. Harbor LM never asks for the network version of these services. Where on-device recognition is not available (for example on older system versions), voice input does not work, and no audio is sent anywhere instead.
- Reading text from photos and scanned pages. On iPhone and iPad this uses Apple's on-device Vision text recognition; on Android it uses an AI model that runs on the device, if you have installed one.

Harbor LM does not include text-to-speech.

## 3. Do conversations, prompts, files or audio leave the device?

No, except an answer you choose to report (section 4). Harbor LM does not otherwise send your conversations, prompts, AI answers, documents, photos, audio recordings or Knowledge index anywhere. There is no cloud inference, no online search of the web and no connected service in this version.

Your operating system works outside Harbor LM's control. For example, Apple or Google may download language packs for their on-device speech and text recognition, and on Android, taking a photo opens your phone's camera app. Those are governed by Apple's or Google's own privacy policies.

## 4. Optional online features

Each of these requests happens only when you act:

- Sign-in: signing in sends your sign-in to Apple or Google and then to Firebase Authentication. Signing in needs an internet connection once; after that the app keeps you signed in on the device and works offline, and Firebase refreshes the sign-in from time to time when you are online.
- Reporting an answer: when you press Send in the Report sheet, the report described in section 1 is sent over an encrypted connection to our report service (harbor-lm-reports.mohsinmustafa.workers.dev, run on Cloudflare). We use reports only to review harmful or offensive output and to improve how the app handles it, and we keep each report for 180 days.

- Search: when you search Hugging Face for models, the words you type are sent to huggingface.co.
- Check fit and install preparation: the app asks huggingface.co for the list of files in the model you selected, to show its size and whether it fits your device.
- Download: after you confirm the download size, the model files are downloaded from huggingface.co and its download servers (cdn-lfs.huggingface.co, cdn-lfs.hf.co, us.aws.cdn.hf.co and cas-bridge.xethub.hf.co).

There are no analytics, advertising identifiers, automatic update checks or other background network requests. Apart from keeping your sign-in current, nothing is sent when the app starts.

## 5. Model downloads and the network requests they make

A model request contains the address of the model or file, standard web headers and a user-agent that names Harbor LM. Your account, sign-in token or other personal identifier is not attached. Like any website, Hugging Face and its download servers can see your device's IP address, the time of the request and which model you asked for. Hugging Face's handling of that information is described in its own privacy policy at https://huggingface.co/privacy.

Models in the built-in Recommended list are checked against a digitally signed catalog, and each of their files is verified with a SHA-256 checksum before it is used. A failed or interrupted download is discarded. Before a download starts, the app checks that enough free storage is available and tells you the size.

Each model is published by its own author under its own license (for example, Apache 2.0 or MIT). For models in the Recommended list the license is shown before you download; for models you find through search, the license is the one on the model's Hugging Face page. Models found through search are not in the signed catalog: they are checked against the size Hugging Face reports, and their checksum is recorded when installed. Harbor LM's AI answers can be inaccurate or incomplete; check important information before relying on it.

## 6. Storage on your device, retention and deletion

What is stored: your conversations, your Knowledge index, saved memories, activity history (which includes the questions you asked), documents you work on, installed models and your app settings. These live in the app's private storage.

Encryption: conversations, the Knowledge index, memories, activity history and diagnostic logs are encrypted on disk with a key held in the iOS Keychain or the Android Keystore. A few things are not encrypted: model files, your display preferences (theme, language and chosen model), the local log of model requests (which records the addresses requested, including search words), and files you choose to save or export. On iPhone and iPad, saved and exported files go to Harbor LM's folder in the Files app, where you and other apps you allow can see them.

Backups: Harbor LM's data folder is excluded from iCloud and computer backups on iOS, and from Google backup and device-to-device transfer on Android.

How long: your content stays on your device until you delete it or uninstall the app. Your account stays until you delete it. Reports are deleted 180 days after they are sent. The diagnostics log keeps only the most recent 500 entries. A model you remove is kept for 72 hours so it can be restored, then deleted.

How to delete:

- A conversation: open the conversation list and choose Delete.
- A document or folder in Knowledge: remove the source. You can also remove the conversation turns that used it.
- A memory: delete it in the Memory panel.
- A model: uninstall it on the Models screen.
- Your account: Settings → Account → Delete account. You sign in once more to confirm; the account is then deleted from Firebase Authentication straight away, and for Apple accounts the app also revokes its access to your Apple ID. Deleting the account does not touch the conversations and files on your device.
- Your reports: email privacy@cogniwiss.com and we will delete them.
- Everything on the device: uninstall Harbor LM. This removes all of the app's data from the device. In this version, activity history cannot be deleted item by item; uninstalling removes it.

## 7. Permissions

- Microphone: used only while you press Record, to turn your speech into text on the device. The recording is deleted as soon as it has been transcribed or you cancel.
- Camera: used only when you choose Take a photo, to read the page on the device. On iPhone and iPad the photo is not saved to your photo library; on Android your camera app takes the photo. In both cases the app deletes its copy after reading it.
- Files: Harbor LM opens only the files you pick with the system file picker. The operating system gives the app a copy to read.
- Internet (Android): needed only for sign-in, reporting and the model features in section 4.

Harbor LM does not request access to your location, contacts, photo library, calendar or other apps, and does not track you across apps or websites.

## 8. Third parties and international transfers

Harbor LM contains no third-party analytics, advertising or crash reporting software. The app contacts these third parties:

- Google LLC (Firebase Authentication and Google Sign-In) stores and processes your account information on our behalf. Its terms and privacy information: https://firebase.google.com/support/privacy.
- Apple Inc. (Sign in with Apple), if you sign in with Apple: https://www.apple.com/legal/privacy/.
- Cloudflare, Inc. runs our report service and stores reports on our behalf: https://www.cloudflare.com/privacypolicy/.
- Hugging Face, Inc., when you use the model features in section 4.

These companies are based in the United States and process data in the United States and other countries where they operate. Where the law requires it, transfers rely on the providers' standard contractual clauses or equivalent safeguards. We do not sell your information and do not share it for advertising.

## 9. Your rights and how to contact us

Depending on where you live (for example under the GDPR in the EU and UK, or the CCPA in California), you may have rights to access, correct, delete or move your personal data, and to object to or restrict its use. The personal data we hold is your account information and any reports you sent. You can delete your account in the app at any time, and you can ask us for a copy of this data or to delete your reports by emailing privacy@cogniwiss.com. We use account information to provide the service you signed up for and reports on the basis of our legitimate interest in keeping the app safe. You may also complain to your local data protection authority. Content on your device is under your control and is described in section 6. For information Hugging Face receives during model downloads, contact Hugging Face.

Children: Harbor LM is a general productivity tool and is not directed at children under 13, and we do not knowingly create accounts for them. If you believe a child has created an account, email us and we will delete it.

Changes: if a future version adds any new data collection or network feature, we will update this policy and the app store privacy declarations before that version is released, and change the effective date above.

Questions or requests: privacy@cogniwiss.com. You can report a harmful or offensive AI answer in the app (the flag on each answer) or by email.

Cogniwiss Labs

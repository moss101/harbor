# Harbor LM: store listing, review notes and privacy forms (draft, 2026-11-03)

Draft text for App Store Connect and Play Console. The claims here are
limited to what `docs/release/store_readiness_2026-11-03.md` verified.
Do not add speed numbers, "works with any model" or "fully offline".

## Names

- App name: **Harbor LM**
- Subtitle (Apple, ≤30): `Private AI on your device`
- Short description (Play, ≤80): `Chat with an AI model that runs on your phone. Private and on-device.`
- Seller / developer: Cogniwiss Labs
- Privacy policy URL: https://harbor-lm.pages.dev/privacy
- Support / contact e-mail: privacy@cogniwiss.com
- Category: Productivity

## Description

Harbor LM runs an AI language model on your iPhone, iPad or Android phone.
Your conversations, documents and recordings are processed on the device
and are never sent to us or to any AI service. The only exception is an
answer you choose to report.

- Chat with an on-device model. Download a model once, then chat without
  a connection.
- Ask about your own files. Open documents, spreadsheets, presentations,
  PDFs and text files and ask questions about them. Answers cite the
  passages they used.
- Create and edit Word, Excel and PowerPoint files.
- Search your notes and files, including images, audio and video, with a
  local index.
- Speak instead of typing, with your phone's on-device speech recognition
  (iOS 26 or later, Android 13 or later).
- Read text from a photo of a page.
- No ads, no analytics. Sign in with Apple or Google; your account never
  holds your conversations or files.

Getting started: Harbor LM downloads its AI model when you first set it
up (about 1 GB for the recommended model; Wi-Fi recommended). The app
shows the size and checks free space before downloading. Which models
fit is shown for your device.

Internet use: the app goes online only when you search for, check or
download a model on the Models screen. Those requests go to Hugging Face.

AI answers can be wrong. Check important information.

English and Arabic interface.

## App Review notes (Apple) / testing instructions (Play)

**Sign-in is required.** Use Sign in with Apple or Google with any
account. A demo account is not needed, because both providers let the
reviewer use their own account. If App Review asks for one, create a
Google account for review and enter it in App Review Information. To try
the AI features after signing in:

1. Open **Models → Recommended** and install **Qwen2.5 1.5B Instruct**
   (about 1.1 GB, downloaded from huggingface.co; the app shows the size
   and asks before it starts).
2. Go to **Home** and ask a question, or attach a document with the
   paperclip and ask about it.
3. Voice input needs iOS 26 or later (Android 13 or later) with on-device
   speech recognition available. It does not work on older versions.
   (Before submitting, check on a device what the app shows there; this
   is NOT VERIFIED.)
4. Settings → About → Privacy Policy shows the policy in the app.

All inference runs on the device. The only network traffic is the model
search, file list and download from Hugging Face in step 1.

## Apple App Privacy (updated 2026-11-03: sign-in and reports)

Data used to track you: **none**. Data linked to you:

| Data type | Collected for | Notes |
|---|---|---|
| Contact Info → Email Address | App Functionality | Firebase Auth (Apple may give a relay address) |
| Contact Info → Name | App Functionality | if the provider shares it |
| Identifiers → User ID | App Functionality | Firebase UID |
| User Content → Other User Content | App Functionality | only reports the user sends (answer, reason, comment, optional question); kept 180 days |

Not collected: location, contacts, health, financial info, browsing or
search history, diagnostics, usage data, and the content of conversations
(which stay on the device).

## Google Play Data safety (updated 2026-11-03)

- Collects data: **Yes**. Shares data: **No**. These are processed on
  the developer's behalf by Firebase and Cloudflare, which Play counts as
  service providers rather than sharing.
- Personal info → **Name, Email address**: collected, required, purpose
  Account management.
- Personal info → **User IDs**: collected, required, purpose Account
  management.
- App activity → **Other user-generated content** (reports): collected,
  optional, purpose App functionality and Fraud prevention, security and
  compliance.
- App activity → **In-app search history** (model search words sent to
  Hugging Face): collected, optional, App functionality, processed
  ephemerally. This is the conservative answer.
- Encrypted in transit: **Yes**.
- Deletion: users can delete their account in the app (Settings →
  Account → Delete account). Account deletion URL for the Play form:
  https://harbor-lm.pages.dev/privacy (§6 explains the in-app path and
  the email address for report deletion).

## Content / age rating inputs

- An AI chatbot that generates text without an output filter. Answer the
  user-generated and AI content questions **yes**. Expect 13+ (Apple) and
  Teen or higher (IARC).
- No ads, no purchases, no location, no account, no contact between
  users.
- Target audience (Play): 13+ or 18+. Not children.

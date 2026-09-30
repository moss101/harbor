// The file-type groups every picker in this app opens with.
//
// These live in one place because iOS — not desktop — sets the contract,
// and it is a contract that fails silently. `file_selector_ios` rejects a
// type group carrying only `extensions`:
//
//     throw ArgumentError('The provided type group $typeGroup should
//       either allow all files, or have a non-empty
//       "uniformTypeIdentifiers"');
//
// It throws from Dart, BEFORE any UIDocumentPickerViewController is
// constructed, so nothing appears on screen and nothing reaches the
// system log. Every call site wraps `openFile` in a catch that treats a
// throw as "the user dismissed the picker" — true on desktop, where the
// only thing that throws is a cancel. On iOS it made all five file
// entry points into buttons that did nothing, every time, with a comment
// at each one asserting the opposite.
//
// So: extensions for desktop (which ignores UTIs), UTIs for iOS/macOS
// (which ignore extensions). A group that omits either is broken on
// half the platforms it ships to, which is what `file_type_groups_test`
// checks against these very values rather than against a copy of them.
import 'package:file_selector/file_selector.dart';

/// Documents the reader/extractors can open: Office, PDF and plain text.
XTypeGroup documentTypeGroup(String label) => XTypeGroup(
      label: label,
      extensions: const ['docx', 'pdf', 'xlsx', 'pptx', 'txt', 'md'],
      uniformTypeIdentifiers: const [
        'org.openxmlformats.wordprocessingml.document',
        'com.adobe.pdf',
        'org.openxmlformats.spreadsheetml.sheet',
        'org.openxmlformats.presentationml.presentation',
        'public.plain-text',
        'net.daringfireball.markdown',
      ],
    );

/// The office-only subset used where a run needs a structured artifact.
XTypeGroup officeTypeGroup(String label) => XTypeGroup(
      label: label,
      extensions: const ['docx', 'pdf', 'xlsx', 'pptx'],
      uniformTypeIdentifiers: const [
        'org.openxmlformats.wordprocessingml.document',
        'com.adobe.pdf',
        'org.openxmlformats.spreadsheetml.sheet',
        'org.openxmlformats.presentationml.presentation',
      ],
    );

/// Knowledge ingest: the document set plus the line-oriented text formats.
XTypeGroup knowledgeTypeGroup(String label) => XTypeGroup(
      label: label,
      extensions: const ['txt', 'md', 'csv', 'json', 'log', 'docx', 'pdf'],
      uniformTypeIdentifiers: const [
        'public.plain-text',
        'net.daringfireball.markdown',
        'public.comma-separated-values-text',
        'public.json',
        'public.log',
        'org.openxmlformats.wordprocessingml.document',
        'com.adobe.pdf',
      ],
    );

/// Local GGUF weights.
///
/// `.gguf` has no registered UTI on any Apple platform — it is not
/// declared by the system and this app does not export it — so there is
/// no identifier that would match the file. Filtering by anything
/// narrower than `public.data` would leave the picker showing the user's
/// own model file greyed out and unselectable, which is a worse failure
/// than no filter: it looks like the file is unsupported. Desktop still
/// gets the `.gguf` extension filter; iOS shows everything and the
/// installer validates the magic bytes.
XTypeGroup modelTypeGroup(String label) => XTypeGroup(
      label: label,
      extensions: const ['gguf'],
      uniformTypeIdentifiers: const ['public.data'],
    );

// Every picker group this app ships must be openable on every platform
// it ships to.
//
// This exists because all five file entry points were dead on iOS for
// as long as they existed. `file_selector_ios` throws ArgumentError on a
// group with no `uniformTypeIdentifiers`, from Dart, before any picker
// is built — so there was no crash, no log line and no empty sheet, just
// a button that did nothing. The catch at each call site recorded the
// cause as "picker dismissed".
//
// The check below reproduces the plugin's own precondition against the
// GROUPS THE APP ACTUALLY PASSES, imported from the same file the
// surfaces import. A copy of the lists here would pass forever while the
// shipped ones rotted.
import 'package:file_selector/file_selector.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/services/file_types.dart';

void main() {
  final groups = <String, XTypeGroup>{
    'document': documentTypeGroup('docs'),
    'office': officeTypeGroup('office'),
    'knowledge': knowledgeTypeGroup('knowledge'),
    'model': modelTypeGroup('models'),
  };

  group('every shipped picker group', () {
    groups.forEach((name, g) {
      test('$name survives the file_selector_ios precondition', () {
        // Verbatim from file_selector_ios: allow everything, or carry a
        // non-empty UTI list. Anything else throws before the picker.
        expect(g.allowsAny || (g.uniformTypeIdentifiers?.isNotEmpty ?? false),
            isTrue,
            reason: 'group "$name" would throw ArgumentError on iOS and the '
                'call site would report it as a dismissal');
      });

      test('$name still filters on desktop', () {
        // Desktop ignores UTIs; a group that dropped its extensions
        // would silently widen to every file on macOS/Windows/Linux.
        expect(g.allowsAny || (g.extensions?.isNotEmpty ?? false), isTrue,
            reason: 'group "$name" has no extension filter for desktop');
      });
    });
  });

  test('model import does not filter itself out of existence on iOS', () {
    // .gguf has no registered UTI. A narrower identifier than public.data
    // greys out the user's own weights in the picker, which reads as
    // "unsupported file" rather than "wrong filter".
    expect(modelTypeGroup('m').uniformTypeIdentifiers, contains('public.data'));
  });
}

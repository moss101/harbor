import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/l10n/app_localizations.dart';
import 'package:harbor_app/surfaces/privacy_policy_page.dart';
import 'package:harbor_ui/harbor_ui.dart';

/// Settings → Privacy Policy shows the same file the public page is built
/// from (tools/build_privacy_site.py), and names the contact the stores
/// list.
void main() {
  final source = File(harborPrivacyPolicyAsset).readAsStringSync();

  test('policy names the publisher, contact and public URL', () {
    expect(source, contains('Cogniwiss Labs'));
    expect(source, contains('privacy@cogniwiss.com'));
    expect(source, contains(harborPrivacyPolicyUrl));
    final blocks = parsePrivacyPolicy(source);
    expect(blocks.first.kind, PolicyBlockKind.title);
    expect(blocks.where((b) => b.kind == PolicyBlockKind.heading).length,
        greaterThanOrEqualTo(9));
  });

  test('the asset is bundled', () {
    expect(File('pubspec.yaml').readAsStringSync(),
        contains('- $harborPrivacyPolicyAsset'));
  });

  testWidgets('page renders the full policy text', (tester) async {
    await tester.pumpWidget(HarborTheme(
      colors: HarborColors.light,
      text: const HarborType(arabic: false),
      child: MaterialApp(
        localizationsDelegates: const [
          AppLocalizations.delegate,
          GlobalMaterialLocalizations.delegate,
          GlobalWidgetsLocalizations.delegate,
          GlobalCupertinoLocalizations.delegate,
        ],
        supportedLocales: const [Locale('en'), Locale('ar')],
        home: PrivacyPolicyPage(bundle: _StringBundle(source)),
      ),
    ));
    await tester.pumpAndSettle();
    expect(find.text('Privacy Policy'), findsOneWidget);
    expect(find.text('Harbor LM Privacy Policy'), findsOneWidget);
    await tester.scrollUntilVisible(find.text(harborPrivacyPolicyUrl), 400);
    expect(find.text(harborPrivacyPolicyUrl), findsOneWidget);
  });
}

class _StringBundle extends CachingAssetBundle {
  _StringBundle(this.text);
  final String text;

  @override
  Future<ByteData> load(String key) async =>
      ByteData.sublistView(Uint8List.fromList(text.codeUnits));

  @override
  Future<String> loadString(String key, {bool cache = true}) async => text;
}

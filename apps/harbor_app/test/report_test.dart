import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/l10n/app_localizations.dart';
import 'package:harbor_app/services/report_service.dart';
import 'package:harbor_app/widgets/report_sheet.dart';
import 'package:harbor_ui/harbor_ui.dart';

import 'sign_in_test.dart' show FakeAuth;

/// The Report sheet sends exactly what it says: the answer, the reason,
/// the comment, and the question only while the switch is on.
void main() {
  test('ReportService posts the bearer token and the confirmed fields',
      () async {
    // flutter_test answers every real HTTP request with 400; this test
    // talks to its own loopback server.
    HttpOverrides.global = null;
    final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    final seen = <Map>[];
    String? authz;
    server.listen((req) async {
      authz = req.headers.value(HttpHeaders.authorizationHeader);
      seen.add(jsonDecode(await utf8.decoder.bind(req).join()) as Map);
      req.response
        ..statusCode = 201
        ..write('{"id":"r1"}');
      await req.response.close();
    });
    final auth = FakeAuth()..signInWithGoogle();
    final svc = ReportService(auth,
        endpoint: Uri.parse('http://127.0.0.1:${server.port}/v1/reports'));
    expect(
        await svc.send(reason: 'harmful', answer: 'A', comment: 'c'), 'r1');
    expect(authz, 'Bearer token');
    expect(seen.single['reason'], 'harmful');
    expect(seen.single['answer'], 'A');
    expect(seen.single.containsKey('question'), isFalse);
    await server.close(force: true);
  });

  testWidgets('switching off the question keeps it out of the report',
      (tester) async {
    final sent = <Map<String, String?>>[];
    final svc = _RecordingService(sent);
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
        home: Scaffold(
          body: ReportSheet(
              service: svc, answer: 'bad answer', question: 'my question'),
        ),
      ),
    ));
    await tester.tap(find.byKey(const ValueKey('report-reason-hateful')));
    await tester.tap(find.byKey(const ValueKey('report-include-question')));
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('report-send')));
    await tester.pump();
    expect(sent.single,
        {'reason': 'hateful', 'answer': 'bad answer', 'question': null});
  });
}

class _RecordingService extends ReportService {
  _RecordingService(this.sent) : super(FakeAuth());
  final List<Map<String, String?>> sent;

  @override
  Future<String> send({
    required String reason,
    required String answer,
    String? question,
    String? comment,
    String? modelId,
  }) async {
    sent.add({'reason': reason, 'answer': answer, 'question': question});
    return 'r1';
  }
}

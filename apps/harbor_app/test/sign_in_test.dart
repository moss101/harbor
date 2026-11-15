import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/l10n/app_localizations.dart';
import 'package:harbor_app/services/auth_service.dart';
import 'package:harbor_app/surfaces/settings_surface.dart';
import 'package:harbor_app/surfaces/sign_in_page.dart';
import 'package:harbor_ui/harbor_ui.dart';

class FakeAuth extends HarborAuth {
  FakeAuth({this.apple = true});
  final bool apple;
  HarborAccount? _account;
  final calls = <String>[];

  @override
  HarborAccount? get account => _account;
  @override
  bool get restoring => false;
  @override
  bool get appleAvailable => apple;

  void _in(String provider) {
    _account = HarborAccount(
        uid: 'u1', email: 'reviewer@example.com', provider: provider);
    notifyListeners();
  }

  @override
  Future<void> signInWithGoogle() async {
    calls.add('google');
    _in('google.com');
  }

  @override
  Future<void> signInWithApple() async {
    calls.add('apple');
    _in('apple.com');
  }

  @override
  Future<void> signOut() async {
    calls.add('signOut');
    _account = null;
    notifyListeners();
  }

  @override
  Future<void> deleteAccount() async {
    calls.add('delete');
    _account = null;
    notifyListeners();
  }

  @override
  Future<String?> idToken() async => _account == null ? null : 'token';
}

Widget _wrap(Widget child) => HarborTheme(
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
        home: Scaffold(body: child),
      ),
    );

void main() {
  testWidgets('iOS offers Apple and Google; Android offers Google only',
      (tester) async {
    final ios = FakeAuth();
    await tester.pumpWidget(_wrap(SignInPage(auth: ios)));
    expect(find.byKey(const ValueKey('sign-in-apple')), findsOneWidget);
    expect(find.byKey(const ValueKey('sign-in-google')), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('sign-in-apple')));
    await tester.pump();
    expect(ios.calls, ['apple']);
    expect(ios.account?.provider, 'apple.com');

    await tester.pumpWidget(_wrap(SignInPage(auth: FakeAuth(apple: false))));
    expect(find.byKey(const ValueKey('sign-in-apple')), findsNothing);
    expect(find.byKey(const ValueKey('sign-in-google')), findsOneWidget);
  });

  testWidgets('account can be deleted from Settings after confirming',
      (tester) async {
    final auth = FakeAuth()..signInWithGoogle();
    await tester.pumpWidget(_wrap(AccountCard(auth: auth)));
    expect(find.text('reviewer@example.com'), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('account-delete')));
    await tester.pumpAndSettle();
    expect(find.text('Delete your account?'), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('account-delete-confirm')));
    await tester.pumpAndSettle();
    expect(auth.calls, ['google', 'delete']);
    expect(auth.account, isNull);
  });
}

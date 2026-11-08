import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math';

import 'package:crypto/crypto.dart';
import 'package:firebase_auth/firebase_auth.dart';
import 'package:flutter/widgets.dart';
import 'package:google_sign_in/google_sign_in.dart';
import 'package:sign_in_with_apple/sign_in_with_apple.dart';

/// The signed-in Harbor LM account, as the UI needs it.
@immutable
class HarborAccount {
  const HarborAccount(
      {required this.uid,
      this.email,
      this.displayName,
      required this.provider});
  final String uid;
  final String? email;
  final String? displayName;

  /// `google.com` or `apple.com`.
  final String provider;
}

/// Sign-in for the store builds (iOS + Android). The account gates the
/// app; it never carries conversations, files or prompts — those stay in
/// the on-device core. Tests and desktop builds run without one.
abstract class HarborAuth extends ChangeNotifier {
  HarborAccount? get account;

  /// True until the persisted session (if any) has been restored.
  bool get restoring;

  bool get appleAvailable;

  Future<void> signInWithGoogle();
  Future<void> signInWithApple();
  Future<void> signOut();

  /// Deletes the account. Signs in again with the same provider first
  /// (Firebase requires a recent sign-in) and, for Apple, revokes the
  /// app's Apple token as Apple requires.
  Future<void> deleteAccount();

  /// A short-lived Firebase ID token for Cogniwiss endpoints (reports).
  Future<String?> idToken();
}

/// User closed the provider sheet: not an error worth showing.
class SignInCancelled implements Exception {
  const SignInCancelled();
}

class FirebaseHarborAuth extends HarborAuth {
  FirebaseHarborAuth() {
    _sub = FirebaseAuth.instance.authStateChanges().listen((u) {
      _user = u;
      _restoring = false;
      notifyListeners();
    });
  }

  late final StreamSubscription<User?> _sub;
  User? _user;
  bool _restoring = true;
  bool _googleReady = false;

  @override
  bool get restoring => _restoring;

  @override
  bool get appleAvailable => Platform.isIOS;

  @override
  HarborAccount? get account {
    final u = _user;
    if (u == null) return null;
    final provider =
        u.providerData.isEmpty ? 'unknown' : u.providerData.first.providerId;
    return HarborAccount(
        uid: u.uid,
        email: u.email,
        displayName: u.displayName,
        provider: provider);
  }

  Future<void> _ensureGoogle() async {
    if (_googleReady) return;
    // Client ids come from GoogleService-Info.plist (iOS, GIDClientID) and
    // google-services.json (Android, default_web_client_id).
    await GoogleSignIn.instance.initialize();
    _googleReady = true;
  }

  Future<OAuthCredential> _googleCredential() async {
    await _ensureGoogle();
    try {
      final acct = await GoogleSignIn.instance.authenticate();
      final idToken = acct.authentication.idToken;
      if (idToken == null) throw StateError('Google returned no ID token');
      return GoogleAuthProvider.credential(idToken: idToken);
    } on GoogleSignInException catch (e) {
      if (e.code == GoogleSignInExceptionCode.canceled) {
        throw const SignInCancelled();
      }
      rethrow;
    }
  }

  /// Apple credential plus the authorization code (needed to revoke).
  Future<(OAuthCredential, String)> _appleCredential() async {
    final rawNonce = _nonce();
    try {
      final apple = await SignInWithApple.getAppleIDCredential(
        scopes: [AppleIDAuthorizationScopes.email],
        nonce: sha256.convert(utf8.encode(rawNonce)).toString(),
      );
      final idToken = apple.identityToken;
      if (idToken == null) throw StateError('Apple returned no identity token');
      return (
        OAuthProvider('apple.com').credential(
          idToken: idToken,
          rawNonce: rawNonce,
          accessToken: apple.authorizationCode,
        ),
        apple.authorizationCode,
      );
    } on SignInWithAppleAuthorizationException catch (e) {
      if (e.code == AuthorizationErrorCode.canceled) {
        throw const SignInCancelled();
      }
      rethrow;
    }
  }

  static String _nonce([int length = 32]) {
    const chars =
        '0123456789ABCDEFGHIJKLMNOPQRSTUVXYZabcdefghijklmnopqrstuvwxyz-._';
    final r = Random.secure();
    return List.generate(length, (_) => chars[r.nextInt(chars.length)]).join();
  }

  @override
  Future<void> signInWithGoogle() async {
    await FirebaseAuth.instance.signInWithCredential(await _googleCredential());
  }

  @override
  Future<void> signInWithApple() async {
    final (cred, _) = await _appleCredential();
    await FirebaseAuth.instance.signInWithCredential(cred);
  }

  @override
  Future<void> signOut() async {
    if (_googleReady) await GoogleSignIn.instance.signOut();
    await FirebaseAuth.instance.signOut();
  }

  @override
  Future<void> deleteAccount() async {
    final u = _user;
    if (u == null) return;
    if (account?.provider == 'apple.com') {
      final (cred, code) = await _appleCredential();
      await u.reauthenticateWithCredential(cred);
      await FirebaseAuth.instance.revokeTokenWithAuthorizationCode(code);
    } else {
      await u.reauthenticateWithCredential(await _googleCredential());
      if (_googleReady) await GoogleSignIn.instance.disconnect();
    }
    await u.delete();
  }

  @override
  Future<String?> idToken() async => _user?.getIdToken();

  @override
  void dispose() {
    _sub.cancel();
    super.dispose();
  }
}

/// Makes the account reachable from Settings (null in tests and on
/// desktop, where the app runs without sign-in).
class AuthScope extends InheritedNotifier<HarborAuth> {
  const AuthScope({super.key, required HarborAuth? auth, required super.child})
      : super(notifier: auth);

  static HarborAuth? maybeOf(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<AuthScope>()?.notifier;
}

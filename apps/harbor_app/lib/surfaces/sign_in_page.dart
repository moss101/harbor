import 'package:flutter/material.dart';
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../services/auth_service.dart';
import 'privacy_policy_page.dart';

/// Required sign-in for the store builds: Sign in with Apple first on iOS
/// (Apple 4.8, offered beside Google), Google on both platforms.
class SignInPage extends StatefulWidget {
  const SignInPage({super.key, required this.auth});
  final HarborAuth auth;

  @override
  State<SignInPage> createState() => _SignInPageState();
}

class _SignInPageState extends State<SignInPage> {
  bool _busy = false;
  String? _error;

  Future<void> _run(Future<void> Function() signIn) async {
    final l10n = AppLocalizations.of(context)!;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await signIn();
    } on SignInCancelled {
      // The user closed the sheet; stay quiet.
    } catch (_) {
      if (mounted) setState(() => _error = l10n.signInFailed);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    return Scaffold(
      backgroundColor: t.colors.canvas,
      body: SafeArea(
        child: Center(
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 420),
            child: ListView(
              shrinkWrap: true,
              padding: const EdgeInsets.all(HarborSpace.s5),
              children: [
                const HarborWordmark(),
                const SizedBox(height: HarborSpace.s2),
                Text(l10n.appTagline,
                    style: t.text.captionOf(t.colors.inkMuted)),
                const SizedBox(height: HarborSpace.s5),
                Text(l10n.signInTitle, style: t.text.titleOf(t.colors.ink)),
                const SizedBox(height: HarborSpace.s2),
                Text(l10n.signInBody, style: t.text.bodyOf(t.colors.ink)),
                const SizedBox(height: HarborSpace.s5),
                if (widget.auth.appleAvailable) ...[
                  FilledButton.icon(
                    key: const ValueKey('sign-in-apple'),
                    style: FilledButton.styleFrom(
                      backgroundColor:
                          t.colors.isDark ? Colors.white : Colors.black,
                      foregroundColor:
                          t.colors.isDark ? Colors.black : Colors.white,
                      minimumSize: const Size.fromHeight(48),
                    ),
                    onPressed:
                        _busy ? null : () => _run(widget.auth.signInWithApple),
                    icon: const Icon(Icons.apple),
                    label: Text(l10n.signInWithApple),
                  ),
                  const SizedBox(height: HarborSpace.s3),
                ],
                OutlinedButton.icon(
                  key: const ValueKey('sign-in-google'),
                  style: OutlinedButton.styleFrom(
                      minimumSize: const Size.fromHeight(48)),
                  onPressed:
                      _busy ? null : () => _run(widget.auth.signInWithGoogle),
                  icon: const Icon(Icons.account_circle_outlined),
                  label: Text(l10n.signInWithGoogle),
                ),
                const SizedBox(height: HarborSpace.s3),
                if (_busy) const LinearProgressIndicator(),
                if (_error != null)
                  Text(_error!, style: t.text.smallOf(t.colors.danger)),
                const SizedBox(height: HarborSpace.s3),
                Text(l10n.signInNeedsNetwork,
                    style: t.text.captionOf(t.colors.inkMuted)),
                Align(
                  alignment: AlignmentDirectional.centerStart,
                  child: TextButton(
                    onPressed: () => Navigator.of(context).push(
                      MaterialPageRoute<void>(
                          builder: (_) => const PrivacyPolicyPage()),
                    ),
                    child: Text(l10n.settingsPrivacyPolicy),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

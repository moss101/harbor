import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';

/// Where the same policy is published (Cloudflare Pages, built from the
/// asset below by tools/build_privacy_site.py). App Store Connect and Play
/// Console link here.
const harborPrivacyPolicyUrl = 'https://harbor-lm.pages.dev/privacy';

/// The policy text, shared with the public page so the two cannot drift.
const harborPrivacyPolicyAsset = 'assets/privacy/privacy_policy.md';

/// One block of the policy: `#`/`##` headings, `- ` bullets, paragraphs.
/// The asset uses no other Markdown, and [parsePrivacyPolicy] is the whole
/// renderer — no Markdown dependency for one document.
class PolicyBlock {
  const PolicyBlock(this.kind, this.text);
  final PolicyBlockKind kind;
  final String text;
}

enum PolicyBlockKind { title, heading, bullet, paragraph }

List<PolicyBlock> parsePrivacyPolicy(String source) {
  final blocks = <PolicyBlock>[];
  for (final raw in source.split('\n')) {
    final line = raw.trimRight();
    if (line.isEmpty) continue;
    if (line.startsWith('## ')) {
      blocks.add(PolicyBlock(PolicyBlockKind.heading, line.substring(3)));
    } else if (line.startsWith('# ')) {
      blocks.add(PolicyBlock(PolicyBlockKind.title, line.substring(2)));
    } else if (line.startsWith('- ')) {
      blocks.add(PolicyBlock(PolicyBlockKind.bullet, line.substring(2)));
    } else {
      blocks.add(PolicyBlock(PolicyBlockKind.paragraph, line));
    }
  }
  return blocks;
}

/// Settings → Privacy Policy: the full policy, readable offline, plus the
/// public address to copy (the app opens no external URLs).
class PrivacyPolicyPage extends StatelessWidget {
  const PrivacyPolicyPage({super.key, this.bundle});

  /// Tests inject a bundle; the app reads [rootBundle].
  final AssetBundle? bundle;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    return Scaffold(
      backgroundColor: t.colors.canvas,
      appBar: AppBar(title: Text(l10n.settingsPrivacyPolicy)),
      body: FutureBuilder<String>(
        future: (bundle ?? rootBundle).loadString(harborPrivacyPolicyAsset),
        builder: (context, snap) {
          if (!snap.hasData) {
            return const Center(child: CircularProgressIndicator());
          }
          final blocks = parsePrivacyPolicy(snap.data!);
          return SelectionArea(
            child: ListView(
              padding: const EdgeInsets.all(HarborSpace.s4),
              children: [
                for (final b in blocks) _block(t, b),
                const SizedBox(height: HarborSpace.s4),
                Row(children: [
                  Expanded(
                    child: Directionality(
                      textDirection: TextDirection.ltr,
                      child: Text(harborPrivacyPolicyUrl,
                          style: t.text.monoOf(t.colors.inkMuted)),
                    ),
                  ),
                  IconButton(
                    tooltip: l10n.copyAction,
                    iconSize: 16,
                    icon: const Icon(Icons.copy_outlined),
                    onPressed: () async {
                      await Clipboard.setData(
                          const ClipboardData(text: harborPrivacyPolicyUrl));
                      if (!context.mounted) return;
                      ScaffoldMessenger.of(context).showSnackBar(
                          SnackBar(content: Text(l10n.copiedMessage)));
                    },
                  ),
                ]),
              ],
            ),
          );
        },
      ),
    );
  }

  Widget _block(HarborTheme t, PolicyBlock b) {
    // The policy is English; keep it left-to-right inside an Arabic UI.
    Widget text(String s, TextStyle style) => Directionality(
        textDirection: TextDirection.ltr,
        child: Text(s, style: style, textAlign: TextAlign.left));
    switch (b.kind) {
      case PolicyBlockKind.title:
        return Padding(
          padding: const EdgeInsets.only(bottom: HarborSpace.s3),
          child: text(b.text, t.text.titleOf(t.colors.ink)),
        );
      case PolicyBlockKind.heading:
        return Padding(
          padding: const EdgeInsets.only(
              top: HarborSpace.s4, bottom: HarborSpace.s2),
          child: text(b.text, t.text.bodyStrongOf(t.colors.ink)),
        );
      case PolicyBlockKind.bullet:
        return Padding(
          padding: const EdgeInsets.only(
              left: HarborSpace.s3, bottom: HarborSpace.s2),
          child: text('•  ${b.text}', t.text.bodyOf(t.colors.ink)),
        );
      case PolicyBlockKind.paragraph:
        return Padding(
          padding: const EdgeInsets.only(bottom: HarborSpace.s2),
          child: text(b.text, t.text.bodyOf(t.colors.ink)),
        );
    }
  }
}

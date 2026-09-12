import 'package:flutter/material.dart';
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import 'surfaces.dart';

/// Ask: grounded Q&A over the workspace. Answers carry citations and the
/// pipeline abstains when evidence is insufficient (goal §12).
class AskSurface extends StatelessWidget {
  const AskSurface({super.key});

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    return Column(children: [
      Expanded(
        child: HarborEmptyState(
          title: l10n.askEmptyTitle,
          body: l10n.askEmptyBody,
        ),
      ),
      SafeArea(
        child: Padding(
          padding: const EdgeInsets.all(HarborSpace.s4),
          child: Composer(hint: l10n.homeComposerHint),
        ),
      ),
    ]);
  }
}

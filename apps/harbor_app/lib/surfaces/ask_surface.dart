import 'package:flutter/material.dart';
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../services/harbor_service.dart';
import 'surfaces.dart';

/// Ask: grounded Q&A over the local Knowledge index. Answers cite real
/// sources with scores and version states; without evidence the surface
/// abstains explicitly (goal §12).
class AskSurface extends StatefulWidget {
  const AskSurface({super.key});

  @override
  State<AskSurface> createState() => _AskSurfaceState();
}

class _AskSurfaceState extends State<AskSurface> {
  final _controller = TextEditingController();
  Map<String, dynamic>? _result;
  bool _searched = false;

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _ask() {
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    if (service == null) return;
    setState(() {
      _result = service.searchKnowledge(_controller.text, topK: 5);
      _searched = true;
    });
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    final knowledgeReady = service != null && service.knowledgeOpen;
    return Column(children: [
      Expanded(
        child: !_searched
            ? HarborEmptyState(
                title: l10n.askEmptyTitle,
                body: knowledgeReady
                    ? l10n.askEmptyBody
                    : 'Knowledge is not open yet. Install an embedding model '
                        '(Models → Installed) to ground answers locally.',
              )
            : _result == null || (_result!['citations'] as List).isEmpty
                ? HarborEmptyState(
                    title: 'No supporting evidence',
                    body: 'Nothing in the local index supports this question, '
                        'so I am abstaining rather than guessing.',
                  )
                : ListView(
                    padding: const EdgeInsets.all(HarborSpace.s4),
                    children: [
                      for (final c in (_result!['citations'] as List).cast<Map>())
                        Builder(builder: (context) {
                          final scorePct =
                              ((c['score'] as num).toDouble() * 100)
                                  .toStringAsFixed(1);
                          final state = c['state'] as String;
                          return Card(
                            margin: const EdgeInsets.only(
                                bottom: HarborSpace.s2),
                            child: ListTile(
                              leading: Icon(Icons.format_quote_outlined,
                                  color: t.colors.brand),
                              title: Text(c['title'] as String),
                              subtitle: Text('score $scorePct% · $state'),
                            ),
                          );
                        }),
                    ],
                  ),
      ),
      SafeArea(
        child: Padding(
          padding: const EdgeInsets.all(HarborSpace.s4),
          child: Row(children: [
            Expanded(
              child: TextField(
                controller: _controller,
                onSubmitted: (_) => _ask(),
                decoration: InputDecoration(hintText: l10n.homeComposerHint),
              ),
            ),
            const SizedBox(width: HarborSpace.s2),
            IconButton.filled(
                onPressed: knowledgeReady ? _ask : null,
                icon: const Icon(Icons.search)),
          ]),
        ),
      ),
    ]);
  }
}

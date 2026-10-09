import 'package:flutter/material.dart';
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../services/harbor_service.dart';

/// Semantic memory (decision 0012): short notes Harbor can recall by
/// meaning, each with the provenance of who wrote it. The user can read,
/// search and delete every record; nothing is remembered silently, and
/// memory never feeds document answers (the core keeps it out of
/// document retrieval).
class MemoryPanel extends StatefulWidget {
  const MemoryPanel({super.key, required this.service});

  final HarborService service;

  @override
  State<MemoryPanel> createState() => _MemoryPanelState();
}

class _MemoryPanelState extends State<MemoryPanel> {
  final _add = TextEditingController();
  final _search = TextEditingController();
  List<Map<String, dynamic>> _all = [];

  /// Non-null while showing search results (best first).
  List<Map<String, dynamic>>? _hits;
  bool _busy = false;

  @override
  void initState() {
    super.initState();
    _reload();
  }

  @override
  void dispose() {
    _add.dispose();
    _search.dispose();
    super.dispose();
  }

  Future<void> _reload() async {
    final all = await widget.service.listMemories();
    if (mounted) setState(() => _all = all);
  }

  void _toast(String message) => ScaffoldMessenger.maybeOf(context)
      ?.showSnackBar(SnackBar(content: Text(message)));

  Future<void> _remember() async {
    final text = _add.text.trim();
    if (text.isEmpty) return;
    final l10n = AppLocalizations.of(context)!;
    setState(() => _busy = true);
    try {
      await widget.service.addMemory(text);
      _add.clear();
      await _reload();
    } catch (e) {
      _toast(l10n.memoryAddFailed);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _find() async {
    final q = _search.text.trim();
    if (q.isEmpty) {
      setState(() => _hits = null);
      return;
    }
    final hits = await widget.service.searchMemories(q);
    if (!mounted) return;
    setState(() => _hits = hits ?? const []);
  }

  Future<void> _delete(Map<String, dynamic> memory) async {
    await widget.service.deleteMemory(memory['id'] as String);
    await _reload();
    if (_hits != null) await _find();
  }

  String _origin(AppLocalizations l10n, Map<String, dynamic> memory) {
    final p = (memory['provenance'] as Map?)?.cast<String, dynamic>() ?? {};
    return switch (p['origin']) {
      'run' => l10n.memoryOriginRun(p['skill_id'] as String? ?? ''),
      'goal' => l10n.memoryOriginGoal,
      _ => l10n.memoryOriginUser,
    };
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final rows = _hits ??
        [
          for (final m in _all) {'memory': m}
        ];
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      HarborSectionHeader(
        title: l10n.memoryHeading,
        trailing: _all.isEmpty ? null : HarborPill('${_all.length}'),
      ),
      Padding(
        padding: const EdgeInsets.only(bottom: HarborSpace.s2),
        child: Text(l10n.memoryExplainer,
            style: t.text.smallOf(t.colors.inkMuted)),
      ),
      HarborCard(
        child:
            Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          TextField(
            key: const ValueKey('memory-add-field'),
            controller: _add,
            maxLines: 2,
            decoration: InputDecoration(hintText: l10n.memoryAddHint),
          ),
          const SizedBox(height: HarborSpace.s2),
          Align(
            alignment: AlignmentDirectional.centerEnd,
            child: FilledButton.icon(
              onPressed: _busy ? null : _remember,
              icon: const Icon(Icons.bookmark_add_outlined, size: 18),
              label: Text(l10n.memoryAddAction),
            ),
          ),
          if (_all.isNotEmpty) ...[
            const SizedBox(height: HarborSpace.s2),
            TextField(
              key: const ValueKey('memory-search-field'),
              controller: _search,
              textInputAction: TextInputAction.search,
              onSubmitted: (_) => _find(),
              decoration: InputDecoration(
                hintText: l10n.memorySearchHint,
                prefixIcon: const Icon(Icons.search),
              ),
            ),
          ],
          if (_all.isEmpty)
            Padding(
              padding: const EdgeInsets.only(top: HarborSpace.s3),
              child: Text(l10n.memoryEmpty,
                  style: t.text.smallOf(t.colors.inkMuted)),
            )
          else if (_hits != null && _hits!.isEmpty)
            Padding(
              padding: const EdgeInsets.only(top: HarborSpace.s3),
              child: Text(l10n.memoryNoMatch,
                  style: t.text.smallOf(t.colors.inkMuted)),
            )
          else
            for (final row in rows) ...[
              const Divider(height: HarborSpace.s4),
              Builder(builder: (context) {
                final m = (row['memory'] as Map).cast<String, dynamic>();
                return HarborListRow(
                  leading: const Icon(Icons.bookmark_outline),
                  title: Text(m['text'] as String? ?? ''),
                  subtitle: Text(_origin(l10n, m)),
                  trailing: IconButton(
                    tooltip: l10n.memoryDeleteAction,
                    icon: const Icon(Icons.delete_outline),
                    onPressed: () => _delete(m),
                  ),
                );
              }),
            ],
        ]),
      ),
    ]);
  }
}

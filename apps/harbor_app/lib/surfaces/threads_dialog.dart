import 'package:flutter/material.dart';
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../services/harbor_service.dart';

/// The conversation history: switch to a saved thread, or delete one
/// (after confirmation — deletion is permanent).
class ThreadsDialog extends StatefulWidget {
  const ThreadsDialog({
    super.key,
    required this.service,
    required this.threads,
    required this.currentId,
    required this.onDeleted,
    required this.onRenamed,
  });
  final HarborService service;
  final List<Map<String, dynamic>> threads;
  final String? currentId;
  final ValueChanged<String> onDeleted;
  final ValueChanged<String> onRenamed;

  @override
  State<ThreadsDialog> createState() => ThreadsDialogState();
}

class ThreadsDialogState extends State<ThreadsDialog> {
  late final List<Map<String, dynamic>> _threads = [...widget.threads];

  Future<void> _rename(Map<String, dynamic> thread) async {
    final id = thread['thread_id'] as String;
    final title = await showDialog<String>(
      context: context,
      builder: (_) =>
          RenameDialog(initial: (thread['title'] as String?) ?? ''),
    );
    // Blank titles are refused by the core; keep the old one.
    if (title == null || title.trim().isEmpty || !mounted) return;
    if (!await widget.service.renameThread(id, title)) return;
    if (!mounted) return;
    setState(() {
      final i = _threads.indexWhere((t) => t['thread_id'] == id);
      if (i >= 0) _threads[i] = {..._threads[i], 'title': title.trim()};
    });
    widget.onRenamed(id);
  }

  Future<void> _delete(Map<String, dynamic> thread) async {
    final l10n = AppLocalizations.of(context)!;
    final colors = HarborTheme.of(context).colors;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(l10n.askDeleteConversationTitle),
        content: Text(l10n.askDeleteConversationBody),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: Text(l10n.cancelAction),
          ),
          FilledButton(
            style: FilledButton.styleFrom(
              backgroundColor: colors.danger,
              foregroundColor: colors.onBrand,
            ),
            onPressed: () => Navigator.of(context).pop(true),
            child: Text(l10n.askDeleteConversationConfirm),
          ),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;
    final id = thread['thread_id'] as String;
    if (!await widget.service.deleteThread(id)) return;
    widget.onDeleted(id);
    if (!mounted) return;
    setState(() => _threads.removeWhere((t) => t['thread_id'] == id));
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final material = MaterialLocalizations.of(context);
    return AlertDialog(
      title: Text(l10n.askConversationsTitle),
      content: SizedBox(
        width: 440,
        child: _threads.isEmpty
            ? Text(l10n.askConversationsEmpty,
                style: t.text.bodyOf(t.colors.inkMuted))
            : ListView.builder(
                shrinkWrap: true,
                itemCount: _threads.length,
                itemBuilder: (context, i) {
                  final th = _threads[i];
                  final title = (th['title'] as String?) ?? '';
                  final when =
                      DateTime.tryParse((th['updated_at'] as String?) ?? '')
                          ?.toLocal();
                  return ListTile(
                    selected: th['thread_id'] == widget.currentId,
                    title: Text(
                        title.isEmpty ? l10n.askConversationUntitled : title,
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis),
                    subtitle: Text([
                      l10n.askConversationQuestions(
                          ((th['turns'] as num?) ?? 0).toInt()),
                      if (when != null) material.formatMediumDate(when),
                    ].join(' · ')),
                    onTap: () =>
                        Navigator.of(context).pop(th['thread_id'] as String),
                    trailing: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        IconButton(
                          tooltip: l10n.askRenameConversation,
                          onPressed: () => _rename(th),
                          icon: const Icon(Icons.edit_outlined),
                        ),
                        IconButton(
                          tooltip: l10n.askDeleteConversation,
                          onPressed: () => _delete(th),
                          icon: const Icon(Icons.delete_outline),
                        ),
                      ],
                    ),
                  );
                },
              ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: Text(l10n.closeAction),
        ),
      ],
    );
  }
}

/// Owns its text controller, so the field is disposed only after the
/// route has finished animating away (disposing it right after `pop`
/// would rebuild the exiting TextField with a dead controller).
class RenameDialog extends StatefulWidget {
  const RenameDialog({super.key, required this.initial});
  final String initial;

  @override
  State<RenameDialog> createState() => RenameDialogState();
}

class RenameDialogState extends State<RenameDialog> {
  late final TextEditingController _controller =
      TextEditingController(text: widget.initial);

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    return AlertDialog(
      title: Text(l10n.askRenameConversation),
      content: TextField(
        controller: _controller,
        autofocus: true,
        maxLength: 80,
        decoration: InputDecoration(hintText: l10n.askRenameHint),
        onSubmitted: (v) => Navigator.of(context).pop(v),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: Text(l10n.cancelAction),
        ),
        FilledButton(
          onPressed: () => Navigator.of(context).pop(_controller.text),
          child: Text(l10n.askRenameSave),
        ),
      ],
    );
  }
}

import 'package:flutter/material.dart';
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../services/auth_service.dart';
import '../services/report_service.dart';

/// Opens the Report sheet for one AI answer. Nothing is sent until the
/// person presses Send; the sheet says exactly what will be sent.
Future<void> showReportSheet(
  BuildContext context, {
  required HarborAuth auth,
  required String answer,
  String? question,
  String? modelId,
  ReportService? service,
}) {
  return showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    builder: (_) => ReportSheet(
      service: service ?? ReportService(auth),
      answer: answer,
      question: question,
      modelId: modelId,
    ),
  );
}

class ReportSheet extends StatefulWidget {
  const ReportSheet({
    super.key,
    required this.service,
    required this.answer,
    this.question,
    this.modelId,
  });
  final ReportService service;
  final String answer;
  final String? question;
  final String? modelId;

  @override
  State<ReportSheet> createState() => _ReportSheetState();
}

class _ReportSheetState extends State<ReportSheet> {
  String _reason = 'offensive';
  bool _includeQuestion = true;
  bool _sending = false;
  String? _error;
  final _comment = TextEditingController();

  @override
  void dispose() {
    _comment.dispose();
    super.dispose();
  }

  String _label(AppLocalizations l10n, String r) => switch (r) {
        'offensive' => l10n.reportReasonOffensive,
        'harmful' => l10n.reportReasonHarmful,
        'sexual' => l10n.reportReasonSexual,
        'hateful' => l10n.reportReasonHateful,
        'illegal' => l10n.reportReasonIllegal,
        'misinformation' => l10n.reportReasonMisinformation,
        _ => l10n.reportReasonOther,
      };

  Future<void> _send() async {
    final l10n = AppLocalizations.of(context)!;
    setState(() {
      _sending = true;
      _error = null;
    });
    try {
      await widget.service.send(
        reason: _reason,
        answer: widget.answer,
        question: _includeQuestion ? widget.question : null,
        comment: _comment.text.trim(),
        modelId: widget.modelId,
      );
      if (!mounted) return;
      final messenger = ScaffoldMessenger.of(context);
      Navigator.of(context).pop();
      messenger.showSnackBar(SnackBar(content: Text(l10n.reportSent)));
    } catch (_) {
      if (mounted) {
        setState(() {
          _sending = false;
          _error = l10n.reportFailed;
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final hasQuestion = (widget.question ?? '').isNotEmpty;
    return Padding(
      padding: EdgeInsets.only(bottom: MediaQuery.viewInsetsOf(context).bottom),
      child: SafeArea(
        child: ListView(
          shrinkWrap: true,
          padding: const EdgeInsets.all(HarborSpace.s4),
          children: [
            Text(l10n.reportTitle, style: t.text.titleOf(t.colors.ink)),
            const SizedBox(height: HarborSpace.s2),
            Text(l10n.reportExplainer,
                style: t.text.smallOf(t.colors.inkMuted)),
            const SizedBox(height: HarborSpace.s3),
            Wrap(
              spacing: HarborSpace.s2,
              runSpacing: HarborSpace.s1,
              children: [
                for (final r in ReportService.reasons)
                  ChoiceChip(
                    key: ValueKey('report-reason-$r'),
                    label: Text(_label(l10n, r)),
                    selected: _reason == r,
                    onSelected:
                        _sending ? null : (_) => setState(() => _reason = r),
                  ),
              ],
            ),
            const SizedBox(height: HarborSpace.s3),
            TextField(
              controller: _comment,
              maxLength: 2000,
              maxLines: 3,
              minLines: 1,
              decoration: InputDecoration(labelText: l10n.reportComment),
            ),
            if (hasQuestion)
              SwitchListTile(
                key: const ValueKey('report-include-question'),
                contentPadding: EdgeInsets.zero,
                title: Text(l10n.reportIncludeQuestion),
                value: _includeQuestion,
                onChanged: _sending
                    ? null
                    : (v) => setState(() => _includeQuestion = v),
              ),
            if (_error != null)
              Padding(
                padding: const EdgeInsets.only(bottom: HarborSpace.s2),
                child: Text(_error!, style: t.text.smallOf(t.colors.danger)),
              ),
            Row(children: [
              TextButton(
                onPressed: _sending ? null : () => Navigator.of(context).pop(),
                child: Text(l10n.cancelAction),
              ),
              const Spacer(),
              FilledButton(
                key: const ValueKey('report-send'),
                onPressed: _sending ? null : _send,
                child: _sending
                    ? const SizedBox(
                        width: 16,
                        height: 16,
                        child: CircularProgressIndicator(strokeWidth: 2))
                    : Text(l10n.reportSend),
              ),
            ]),
          ],
        ),
      ),
    );
  }
}

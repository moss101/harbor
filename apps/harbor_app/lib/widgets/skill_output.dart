import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import 'answer.dart' show AnswerText;

/// A skill run's outputs, read like a document rather than a JSON dump: a
/// header/title field becomes the title, a list of strings becomes a short
/// headed bullet list, a string becomes a paragraph (text results keep a
/// Copy action — Harbor never sends them itself). Anything deeper falls
/// back to the exact JSON, so nothing is hidden or reinterpreted.
class SkillOutputView extends StatelessWidget {
  const SkillOutputView(
      {super.key, required this.outputs, this.preview = false});

  /// The run status's `outputs` map, as the core returned it.
  final Map outputs;

  /// The conversation's card: no copy actions, long text clipped.
  final bool preview;

  static const _titleKeys = {'header', 'title', 'subject', 'name'};

  /// `next_steps` → `Next steps`, `rendered.subject` → `Subject`. Keys
  /// are schema data (English).
  static String humanize(String key) {
    final leaf = key.contains('.') ? key.split('.').last : key;
    final s = leaf.replaceAll(RegExp(r'[_\-]+'), ' ').trim();
    return s.isEmpty ? key : s[0].toUpperCase() + s.substring(1);
  }

  @override
  Widget build(BuildContext context) {
    final entries = outputs.entries.toList();
    return Column(
      key: const ValueKey('skill-output'),
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        for (final (i, e) in entries.indexed) ...[
          if (i > 0) const SizedBox(height: HarborSpace.s4),
          _output(context, e.key.toString(), e.value,
              single: entries.length == 1),
        ],
      ],
    );
  }

  Widget _output(BuildContext context, String label, Object? value,
      {required bool single}) {
    final t = HarborTheme.of(context);
    final l10n = AppLocalizations.of(context)!;
    if (value is String) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(children: [
            Expanded(
              child: Text(humanize(label),
                  style: t.text.captionOf(t.colors.inkMuted)),
            ),
            if (!preview)
              TextButton.icon(
                key: ValueKey('copy-$label'),
                onPressed: () async {
                  HapticFeedback.selectionClick();
                  await Clipboard.setData(ClipboardData(text: value));
                  if (!context.mounted) return;
                  ScaffoldMessenger.maybeOf(context)?.showSnackBar(
                      SnackBar(content: Text(l10n.skillsCopied)));
                },
                icon: const Icon(Icons.copy_rounded, size: 16),
                label: Text(l10n.skillsCopy),
              ),
          ]),
          const SizedBox(height: HarborSpace.s1),
          _prose(t, value),
        ],
      );
    }
    if (value is Map && _readable(value)) {
      final title = value.entries
          .where((e) => _titleKeys.contains(e.key) && e.value is String)
          .map((e) => e.value as String)
          .firstOrNull;
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (!single)
            Text(humanize(label), style: t.text.captionOf(t.colors.inkMuted)),
          if (title != null) ...[
            Text(title, style: t.text.h2Of(t.colors.ink)),
            const SizedBox(height: HarborSpace.s2),
          ],
          for (final e in value.entries)
            if (!(_titleKeys.contains(e.key) && e.value is String))
              Padding(
                padding: const EdgeInsets.only(top: HarborSpace.s2),
                child: _field(t, e.key.toString(), e.value),
              ),
        ],
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(humanize(label), style: t.text.captionOf(t.colors.inkMuted)),
        const SizedBox(height: HarborSpace.s1),
        _json(t, value),
      ],
    );
  }

  /// A map is shown as sections when every value is a scalar or a list of
  /// scalars; anything nested keeps the JSON view.
  static bool _readable(Map m) => m.values.every((v) =>
      v == null ||
      v is String ||
      v is num ||
      v is bool ||
      (v is List && v.every((x) => x is String || x is num || x is bool)));

  Widget _field(HarborTheme t, String key, Object? v) {
    final heading =
        Text(humanize(key), style: t.text.bodyStrongOf(t.colors.ink));
    if (v is List) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          heading,
          const SizedBox(height: HarborSpace.s1),
          for (final item in v)
            Padding(
              padding: const EdgeInsets.only(bottom: HarborSpace.s1),
              child:
                  Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
                Padding(
                  padding: const EdgeInsetsDirectional.only(
                      start: HarborSpace.s1, end: HarborSpace.s2, top: 9),
                  child: Container(
                    width: 5,
                    height: 5,
                    decoration: BoxDecoration(
                        color: t.colors.inkMuted, shape: BoxShape.circle),
                  ),
                ),
                Expanded(
                  child: Text('$item',
                      maxLines: preview ? 3 : null,
                      overflow: preview ? TextOverflow.ellipsis : null,
                      style: t.text.bodyOf(t.colors.ink).copyWith(height: 1.5)),
                ),
              ]),
            ),
          if (v.isEmpty) Text('—', style: t.text.bodyOf(t.colors.inkMuted)),
        ],
      );
    }
    if (v is String && v.length > 60) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          heading,
          const SizedBox(height: HarborSpace.s1),
          _prose(t, v)
        ],
      );
    }
    return HarborKeyValue(label: humanize(key), value: v == null ? '—' : '$v');
  }

  Widget _prose(HarborTheme t, String text) => preview
      ? Text(text,
          maxLines: 6,
          overflow: TextOverflow.ellipsis,
          style: t.text.bodyOf(t.colors.ink).copyWith(height: 1.5))
      : AnswerText(text);

  Widget _json(HarborTheme t, Object? value) => Container(
        width: double.infinity,
        padding: const EdgeInsets.all(HarborSpace.s3),
        decoration: BoxDecoration(
          color: t.colors.surfaceRaised,
          borderRadius: BorderRadius.circular(HarborRadius.sm),
          border: Border.all(color: t.colors.borderSubtle),
        ),
        child: Directionality(
          textDirection: TextDirection.ltr,
          child: SelectableText(
              const JsonEncoder.withIndent('  ').convert(value),
              maxLines: preview ? 8 : null,
              style: t.text.monoOf(t.colors.ink, size: 12)),
        ),
      );
}

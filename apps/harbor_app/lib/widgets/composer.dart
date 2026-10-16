import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../services/request_router.dart' show RequestMode;
import '../shell/keyboard.dart';

/// What a request is about, shown as a chip at the start of the field so
/// the person can always tell what an answer will be grounded in.
enum ComposerScopeKind { everything, thisFile, collection }

final class ComposerScope {
  const ComposerScope.everything()
      : kind = ComposerScopeKind.everything,
        name = null;
  const ComposerScope.thisFile(String this.name)
      : kind = ComposerScopeKind.thisFile;
  const ComposerScope.collection(String this.name)
      : kind = ComposerScopeKind.collection;

  final ComposerScopeKind kind;

  /// File or collection name (data, shown as is).
  final String? name;

  String label(AppLocalizations l10n) => switch (kind) {
        ComposerScopeKind.everything => l10n.composerScopeEverything,
        ComposerScopeKind.thisFile => l10n.composerScopeThisFile(name ?? ''),
        ComposerScopeKind.collection =>
          l10n.composerScopeCollection(name ?? ''),
      };

  IconData get icon => switch (kind) {
        ComposerScopeKind.everything => Icons.public_outlined,
        ComposerScopeKind.thisFile => Icons.description_outlined,
        ComposerScopeKind.collection => Icons.folder_outlined,
      };
}

/// The request composer shared by Home and the Work command bar: text plus
/// optional scope chip, attachment chips, Ask / Change toggle and attach /
/// scan / voice actions. It only collects a request; [onSubmit] decides
/// what happens to it. Desktop: Enter sends, Shift+Enter inserts a
/// newline; mobile: the keyboard's send key.
class HarborComposer extends StatefulWidget {
  const HarborComposer({
    super.key,
    required this.hint,
    this.onSubmit,
    this.controller,
    this.focusNode,
    this.onAttach,
    this.onScan,
    this.onRecord,
    this.scope,
    this.mode,
    this.onModeChanged,
    this.attachments = const [],
    this.busy = false,
    this.onCancel,
    this.elevated = true,
  });
  final String hint;

  /// Called with the submitted (trimmed) text; the field is then cleared.
  final ValueChanged<String>? onSubmit;

  /// Optional external controller/focus so quick actions can prefill.
  final TextEditingController? controller;
  final FocusNode? focusNode;

  /// Attach action. The button is shown whenever any of attach / scan /
  /// record is wired (visibly disabled when only this one is null); a
  /// composer with none of them (the Work bar) has no attach row.
  final VoidCallback? onAttach;

  /// Read a scan or photo / dictate or transcribe a recording; each ends in a
  /// review step and fills the composer, never submits. Hidden when null.
  final VoidCallback? onScan;
  final VoidCallback? onRecord;

  /// Scope chip at the start of the field; none when null.
  final ComposerScope? scope;

  /// Ask / Change toggle, shown when [onModeChanged] is set. A null [mode]
  /// means "let the router decide"; tapping the selected mode clears it.
  final RequestMode? mode;
  final ValueChanged<RequestMode?>? onModeChanged;

  /// Chips for files attached to the request (above the field).
  final List<Widget> attachments;

  /// While a request runs, sending is disabled and [onCancel] (when set)
  /// replaces the send button with a stop button.
  final bool busy;
  final VoidCallback? onCancel;

  /// A soft shadow (Home); docked bars sit flat on their surface.
  final bool elevated;

  @override
  State<HarborComposer> createState() => _HarborComposerState();
}

/// The original name, kept for existing callers and tests.
typedef Composer = HarborComposer;

class _HarborComposerState extends State<HarborComposer> {
  TextEditingController? _ownedController;

  TextEditingController get _controller =>
      widget.controller ?? (_ownedController ??= TextEditingController());

  @override
  void initState() {
    super.initState();
    _controller.addListener(_onChanged);
  }

  @override
  void didUpdateWidget(HarborComposer old) {
    super.didUpdateWidget(old);
    if (!identical(old.controller, widget.controller)) {
      (old.controller ?? _ownedController)?.removeListener(_onChanged);
      _controller.addListener(_onChanged);
    }
  }

  void _onChanged() {
    if (mounted) setState(() {});
  }

  @override
  void dispose() {
    _controller.removeListener(_onChanged);
    // Only dispose a controller the composer itself created.
    _ownedController?.dispose();
    super.dispose();
  }

  void _send() {
    if (widget.busy) return;
    final text = _controller.text.trim();
    if (text.isEmpty) return;
    widget.onSubmit?.call(text);
    _controller.clear();
  }

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final l10n = AppLocalizations.of(context)!;
    final compact = HarborBreakpoints.isCompact(HarborBreakpoints.of(context));
    final canSend = !widget.busy && _controller.text.trim().isNotEmpty;
    final field = TextField(
      key: const ValueKey('composer-field'),
      controller: _controller,
      focusNode: widget.focusNode,
      minLines: 1,
      maxLines: 6,
      textInputAction: harborHasKeyboardShortcuts
          ? TextInputAction.newline
          : TextInputAction.send,
      onSubmitted: (_) => _send(),
      style: t.text.bodyOf(t.colors.ink).copyWith(fontSize: 15),
      decoration: InputDecoration(
        hintText: widget.hint,
        border: InputBorder.none,
        enabledBorder: InputBorder.none,
        focusedBorder: InputBorder.none,
        filled: false,
        contentPadding: const EdgeInsets.symmetric(
            horizontal: HarborSpace.s2, vertical: HarborSpace.s3),
      ),
    );
    final input = harborHasKeyboardShortcuts
        ? CallbackShortcuts(
            bindings: {
              const SingleActivator(LogicalKeyboardKey.enter): _send,
              const SingleActivator(LogicalKeyboardKey.numpadEnter): _send,
            },
            child: field,
          )
        : field;
    final scope = widget.scope;
    final scopeChip = scope == null
        ? null
        : ConstrainedBox(
            constraints: BoxConstraints(maxWidth: compact ? 280 : 240),
            child: Tooltip(
              message: scope.label(l10n),
              child: HarborPill(
                scope.label(l10n),
                key: const ValueKey('composer-scope'),
                icon: scope.icon,
                brand: true,
              ),
            ),
          );
    final showAttach = widget.onAttach != null ||
        widget.onScan != null ||
        widget.onRecord != null;
    final Widget sendButton;
    if (widget.busy && widget.onCancel != null) {
      sendButton = IconButton.filled(
        key: const ValueKey('composer-cancel'),
        tooltip: l10n.cancelAction,
        onPressed: widget.onCancel,
        icon: const Icon(Icons.stop),
      );
    } else if (compact) {
      sendButton = IconButton.filled(
        key: const ValueKey('composer-send'),
        tooltip: l10n.sendAction,
        onPressed: canSend ? _send : null,
        icon: const Icon(Icons.arrow_upward),
      );
    } else {
      sendButton = FilledButton.icon(
        key: const ValueKey('composer-send'),
        onPressed: canSend ? _send : null,
        icon: const Icon(Icons.arrow_forward, size: 16),
        label: Text(l10n.sendAction),
      );
    }
    return Container(
      padding: const EdgeInsets.fromLTRB(
          HarborSpace.s2, HarborSpace.s1, HarborSpace.s2, HarborSpace.s2),
      decoration: ShapeDecoration(
        color: t.colors.surface,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(HarborRadius.lg),
          side: BorderSide(color: t.colors.border),
        ),
        shadows: [
          if (widget.elevated)
            BoxShadow(
              color: t.colors.ink.withValues(alpha: t.isDark ? 0.0 : 0.04),
              blurRadius: 12,
              offset: const Offset(0, 4),
            ),
        ],
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          // Phones: the scope chip leads the chip row above the field, so
          // the field keeps the full width. Wider: it sits at the field's
          // start edge.
          if (widget.attachments.isNotEmpty || (compact && scopeChip != null))
            Padding(
              padding: const EdgeInsets.fromLTRB(
                  HarborSpace.s1, HarborSpace.s2, HarborSpace.s1, 0),
              child: Wrap(
                spacing: HarborSpace.s2,
                runSpacing: HarborSpace.s1,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  if (compact && scopeChip != null) scopeChip,
                  ...widget.attachments,
                ],
              ),
            ),
          if (scopeChip == null || compact)
            input
          else
            Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Padding(
                padding: const EdgeInsetsDirectional.only(
                    start: HarborSpace.s1, top: HarborSpace.s3 - 1),
                child: scopeChip,
              ),
              Expanded(child: input),
            ]),
          Row(children: [
            if (showAttach)
              IconButton(
                tooltip: l10n.attachFilesTooltip,
                onPressed: widget.onAttach,
                icon: const Icon(Icons.attach_file_outlined),
              ),
            if (widget.onScan != null)
              IconButton(
                key: const ValueKey('composer-scan'),
                tooltip: l10n.captureScanTooltip,
                onPressed: widget.onScan,
                icon: const Icon(Icons.document_scanner_outlined),
              ),
            if (widget.onRecord != null)
              IconButton(
                key: const ValueKey('composer-record'),
                tooltip: l10n.captureRecordTooltip,
                onPressed: widget.onRecord,
                icon: const Icon(Icons.mic_none),
              ),
            if (widget.onModeChanged != null)
              Flexible(
                child: Padding(
                  padding: const EdgeInsetsDirectional.only(
                      start: HarborSpace.s1, end: HarborSpace.s2),
                  child: _ModeToggle(
                    mode: widget.mode,
                    onChanged: widget.onModeChanged!,
                    compact: compact,
                  ),
                ),
              ),
            const Spacer(),
            sendButton,
          ]),
        ],
      ),
    );
  }
}

/// Ask / Change. Nothing selected = automatic (the router picks).
class _ModeToggle extends StatelessWidget {
  const _ModeToggle(
      {required this.mode, required this.onChanged, required this.compact});
  final RequestMode? mode;
  final ValueChanged<RequestMode?> onChanged;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final toggle = SegmentedButton<RequestMode>(
      key: const ValueKey('composer-mode'),
      emptySelectionAllowed: true,
      showSelectedIcon: false,
      style: const ButtonStyle(
        visualDensity: VisualDensity.compact,
        tapTargetSize: MaterialTapTargetSize.shrinkWrap,
      ),
      segments: [
        ButtonSegment(
          value: RequestMode.ask,
          icon: const Icon(Icons.question_answer_outlined, size: 16),
          label: compact ? null : Text(l10n.composerModeAsk),
          tooltip: compact ? l10n.composerModeAsk : null,
        ),
        ButtonSegment(
          value: RequestMode.change,
          icon: const Icon(Icons.edit_note_outlined, size: 16),
          label: compact ? null : Text(l10n.composerModeChange),
          tooltip: compact ? l10n.composerModeChange : null,
        ),
      ],
      selected: {if (mode != null) mode!},
      onSelectionChanged: (s) => onChanged(s.isEmpty ? null : s.first),
    );
    // Nothing selected is a real state (the router picks); say so.
    if (mode != null) return toggle;
    return Tooltip(message: l10n.composerModeAutoTooltip, child: toggle);
  }
}

import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:harbor_native/harbor_ffi.dart' as ffi;
import 'package:harbor_ui/harbor_ui.dart';

import '../../l10n/app_localizations.dart';
import '../../services/harbor_service.dart';

/// Excel column letters for a 1-based column index.
String columnLetter(int col) {
  var c = col;
  final buf = <int>[];
  while (c > 0) {
    final rem = (c - 1) % 26;
    buf.insert(0, 65 + rem);
    c = (c - 1) ~/ 26;
  }
  return String.fromCharCodes(buf);
}

/// One preview cell (1-based row/col, formula without '=', cached value).
class GridCell {
  const GridCell(
      {required this.row, required this.col, this.formula, this.value});
  final int row;
  final int col;
  final String? formula;
  final String? value;

  String get ref => '${columnLetter(col)}$row';
  bool get hasFormula => formula != null && formula!.isNotEmpty;
}

/// Spreadsheet workspace (UX-009): grid with frozen row/column headers,
/// formula bar bound to the selected cell, sheet tabs and a cached-value
/// disclaimer (values are never presented as verified by themselves).
class WorkbookView extends StatefulWidget {
  const WorkbookView({super.key, required this.preview});
  final Map preview;

  @override
  State<WorkbookView> createState() => _WorkbookViewState();
}

class _WorkbookViewState extends State<WorkbookView> {
  static const int maxRows = 400;
  static const int maxCols = 60;
  static const double rowHeight = 30;
  static const double headerWidth = 48;
  static const double colWidth = 112;

  late Map<(int, int), GridCell> _cells;
  late int _rows;
  late int _cols;
  (int, int)? _selected;
  bool _showFormulas = false;
  bool _savingEdit = false;
  String? _editError;

  final _hScroll = ScrollController();
  final _bodyV = ScrollController();
  final _headerV = ScrollController();
  bool _syncing = false;

  @override
  void initState() {
    super.initState();
    _index();
    _bodyV.addListener(() => _sync(_bodyV, _headerV));
    _headerV.addListener(() => _sync(_headerV, _bodyV));
  }

  @override
  void didUpdateWidget(covariant WorkbookView oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(oldWidget.preview, widget.preview)) _index();
  }

  void _sync(ScrollController from, ScrollController to) {
    if (_syncing || !to.hasClients || !from.hasClients) return;
    if ((to.offset - from.offset).abs() < 0.5) return;
    _syncing = true;
    to.jumpTo(from.offset.clamp(0, to.position.maxScrollExtent));
    _syncing = false;
  }

  void _index() {
    final raw = (widget.preview['cells'] as List? ?? const []).cast<Map>();
    _cells = {};
    var maxRow = 0;
    var maxCol = 0;
    GridCell? firstFormula;
    for (final c in raw) {
      final cell = GridCell(
        row: (c['row'] as num).toInt(),
        col: (c['col'] as num).toInt(),
        formula: c['formula'] as String?,
        value: c['value']?.toString(),
      );
      _cells[(cell.row, cell.col)] = cell;
      maxRow = math.max(maxRow, cell.row);
      maxCol = math.max(maxCol, cell.col);
      if (firstFormula == null && cell.hasFormula) firstFormula = cell;
    }
    _rows = math.min(math.max(maxRow + 2, 20), maxRows);
    _cols = math.min(math.max(maxCol + 1, 8), maxCols);
    // Default selection: the first formula (the most informative cell)
    // so the formula bar explains something immediately; else A1.
    _selected =
        firstFormula == null ? (1, 1) : (firstFormula.row, firstFormula.col);
  }

  @override
  void dispose() {
    _hScroll.dispose();
    _bodyV.dispose();
    _headerV.dispose();
    super.dispose();
  }

  /// Commit one cell edit through the core's typed edit path: '=' input
  /// becomes a formula, a parseable number a number, empty a blank,
  /// anything else text. The core recalculates with the pinned engine and
  /// the refreshed preview replaces this view's data.
  Future<void> _commitEdit(String input) async {
    if (_selected == null || _savingEdit) return;
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    if (service == null || sp.failed) return;
    final sheet = widget.preview['sheet'] as String? ?? '';
    final trimmed = input.trim();
    final kind = trimmed.startsWith('=')
        ? 'formula'
        : trimmed.isEmpty
            ? 'blank'
            : double.tryParse(trimmed) != null
                ? 'number'
                : 'text';
    final value = kind == 'blank' ? null : trimmed;
    setState(() {
      _savingEdit = true;
      _editError = null;
    });
    try {
      await service.editWorkbookCells([
        {
          'sheet': sheet,
          'row': _selected!.$1,
          'col': _selected!.$2,
          'kind': kind,
          if (value != null) 'value': value,
        },
      ]);
    } on ffi.HarborCoreException catch (e) {
      if (mounted) setState(() => _editError = e.message);
    } finally {
      if (mounted) setState(() => _savingEdit = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final sheets =
        (widget.preview['sheets'] as List? ?? const []).cast<String>();
    final sheet = widget.preview['sheet'] as String? ?? '';
    final chartCount = (widget.preview['chart_count'] as num?)?.toInt() ?? 0;
    final selected = _selected == null ? null : _cells[_selected!];
    final selectedRef = _selected == null
        ? ''
        : '${columnLetter(_selected!.$2)}${_selected!.$1}';
    final scale = MediaQuery.textScalerOf(context).scale(1.0).clamp(1.0, 2.0);
    final rowH = rowHeight * scale;
    final colW = colWidth * scale;
    final headerW = headerWidth * scale;

    // Self-sufficient chrome: chips and ink need a Material ancestor even
    // when the canvas is hosted outside a Scaffold.
    return Material(
      type: MaterialType.transparency,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          _FormulaBar(
            cellRef: selectedRef,
            cell: selected,
            showFormulas: _showFormulas,
            onToggleFormulas: (v) => setState(() => _showFormulas = v),
            onCommit: _commitEdit,
            saving: _savingEdit,
            error: _editError,
          ),
          Expanded(
            child: Container(
              decoration: BoxDecoration(
                color: t.colors.surface,
                border: Border(top: BorderSide(color: t.colors.border)),
              ),
              child: Scrollbar(
                controller: _hScroll,
                thumbVisibility: true,
                notificationPredicate: (n) => n.metrics.axis == Axis.horizontal,
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    // Frozen row-number column (scrolls vertically in sync).
                    SizedBox(
                      width: headerW,
                      child: Column(children: [
                        _HeaderCell(width: headerW, height: rowH, label: ''),
                        Expanded(
                          child: ScrollConfiguration(
                            behavior: ScrollConfiguration.of(context)
                                .copyWith(scrollbars: false),
                            child: ListView.builder(
                              controller: _headerV,
                              itemExtent: rowH,
                              itemCount: _rows,
                              itemBuilder: (context, i) => _HeaderCell(
                                width: headerW,
                                height: rowH,
                                label: '${i + 1}',
                                highlighted: _selected?.$1 == i + 1,
                              ),
                            ),
                          ),
                        ),
                      ]),
                    ),
                    Expanded(
                      child: SingleChildScrollView(
                        controller: _hScroll,
                        scrollDirection: Axis.horizontal,
                        child: SizedBox(
                          width: colW * _cols,
                          child: Column(children: [
                            // Frozen column-letter header.
                            Row(children: [
                              for (var c = 1; c <= _cols; c++)
                                _HeaderCell(
                                  width: colW,
                                  height: rowH,
                                  label: columnLetter(c),
                                  highlighted: _selected?.$2 == c,
                                ),
                            ]),
                            Expanded(
                              child: ListView.builder(
                                controller: _bodyV,
                                itemExtent: rowH,
                                itemCount: _rows,
                                itemBuilder: (context, i) {
                                  final r = i + 1;
                                  return Row(children: [
                                    for (var c = 1; c <= _cols; c++)
                                      _GridCellView(
                                        width: colW,
                                        height: rowH,
                                        cell: _cells[(r, c)],
                                        selected: _selected == (r, c),
                                        showFormula: _showFormulas,
                                        onTap: () =>
                                            setState(() => _selected = (r, c)),
                                      ),
                                  ]);
                                },
                              ),
                            ),
                          ]),
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
          _SheetTabs(
            sheets: sheets,
            active: sheet,
            chartCount: chartCount,
            cellCount: _cells.length,
            l10n: l10n,
          ),
        ],
      ),
    );
  }
}

class _FormulaBar extends StatefulWidget {
  const _FormulaBar({
    required this.cellRef,
    required this.cell,
    required this.showFormulas,
    required this.onToggleFormulas,
    required this.onCommit,
    required this.saving,
    this.error,
  });
  final String cellRef;
  final GridCell? cell;
  final bool showFormulas;
  final ValueChanged<bool> onToggleFormulas;
  final ValueChanged<String> onCommit;
  final bool saving;
  final String? error;

  @override
  State<_FormulaBar> createState() => _FormulaBarState();
}

class _FormulaBarState extends State<_FormulaBar> {
  final _controller = TextEditingController();
  final _focus = FocusNode();
  (int, int)? _editedFor;

  @override
  void dispose() {
    _controller.dispose();
    _focus.dispose();
    super.dispose();
  }

  /// Load the selected cell's content into the input only when the
  /// selection actually changes — never while the user is typing.
  void _syncInput() {
    final cell = widget.cell;
    final key = cell == null ? null : (cell.row, cell.col);
    if (key != _editedFor) {
      _editedFor = key;
      _controller.text = cell == null
          ? ''
          : cell.hasFormula
              ? '=${cell.formula}'
              : (cell.value ?? '');
    }
  }

  void _commit() {
    widget.onCommit(_controller.text);
    _focus.requestFocus();
  }

  @override
  Widget build(BuildContext context) {
    _syncInput();
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final cell = widget.cell;
    final value = cell?.value;
    final wc = HarborBreakpoints.of(context);
    final compact = HarborBreakpoints.isCompact(wc);
    return Container(
      padding: EdgeInsets.symmetric(
          horizontal: HarborBreakpoints.gutter(wc), vertical: HarborSpace.s2),
      color: t.colors.surfaceRaised,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(children: [
            Container(
              constraints: const BoxConstraints(minWidth: 56),
              padding: const EdgeInsets.symmetric(
                  horizontal: HarborSpace.s2, vertical: HarborSpace.s1 + 2),
              decoration: ShapeDecoration(
                color: t.colors.surface,
                shape: RoundedRectangleBorder(
                  borderRadius: BorderRadius.circular(HarborRadius.sm),
                  side: BorderSide(color: t.colors.border),
                ),
              ),
              child: Center(
                child: HarborIdentifier(
                    widget.cellRef.isEmpty ? '—' : widget.cellRef,
                    size: 12,
                    color: t.colors.brand),
              ),
            ),
            const SizedBox(width: HarborSpace.s2),
            Text('fx',
                style: t.text.monoOf(t.colors.inkMuted, size: 12, weight: 600)),
            const SizedBox(width: HarborSpace.s2),
            Expanded(
              child: TextField(
                controller: _controller,
                focusNode: _focus,
                enabled: cell != null && !widget.saving,
                minLines: 1,
                maxLines: 1,
                style: t.text.monoOf(t.colors.ink, size: 13),
                decoration: InputDecoration(
                  isDense: true,
                  hintText: l10n.workEditHint,
                  border: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(HarborRadius.sm),
                    borderSide: BorderSide(color: t.colors.border),
                  ),
                ),
                onSubmitted: (_) => _commit(),
              ),
            ),
            const SizedBox(width: HarborSpace.s2),
            IconButton(
              tooltip: l10n.workEditCommit,
              onPressed: cell == null || widget.saving ? null : _commit,
              icon: widget.saving
                  ? const SizedBox(
                      width: 16,
                      height: 16,
                      child: CircularProgressIndicator(strokeWidth: 2))
                  : const Icon(Icons.check),
            ),
            if (!compact) ...[
              const SizedBox(width: HarborSpace.s2),
              FilterChip(
                label: Text(l10n.workShowFormulas),
                selected: widget.showFormulas,
                onSelected: widget.onToggleFormulas,
                avatar: Icon(Icons.functions, size: 16, color: t.colors.brand),
              ),
            ],
          ]),
          if (value != null || widget.error != null)
            Padding(
              padding: const EdgeInsets.only(top: HarborSpace.s2),
              child: Row(children: [
                if (widget.error != null)
                  Flexible(
                    child: Text(widget.error!,
                        style: t.text.captionOf(t.colors.statusDangerText),
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis),
                  )
                else ...[
                  Text('${l10n.workValue}: ',
                      style: t.text.captionOf(t.colors.inkMuted)),
                  Flexible(
                    child: Text(value ?? '',
                        style: t.text.captionOf(t.colors.ink),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis),
                  ),
                  const SizedBox(width: HarborSpace.s2),
                  Flexible(
                    child: Text('· ${l10n.workCellUnverified}',
                        style: t.text.captionOf(t.colors.statusHybridText),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis),
                  ),
                ],
              ]),
            ),
          if (compact)
            Padding(
              padding: const EdgeInsets.only(top: HarborSpace.s2),
              child: Align(
                alignment: AlignmentDirectional.centerStart,
                child: FilterChip(
                  label: Text(l10n.workShowFormulas),
                  selected: widget.showFormulas,
                  onSelected: widget.onToggleFormulas,
                  avatar:
                      Icon(Icons.functions, size: 16, color: t.colors.brand),
                ),
              ),
            ),
        ],
      ),
    );
  }
}

class _HeaderCell extends StatelessWidget {
  const _HeaderCell({
    required this.width,
    required this.height,
    required this.label,
    this.highlighted = false,
  });
  final double width, height;
  final String label;
  final bool highlighted;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    return Container(
      width: width,
      height: height,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: highlighted ? t.colors.brandSoft : t.colors.surfaceRaised,
        border: Border(
          right: BorderSide(color: t.colors.border),
          bottom: BorderSide(color: t.colors.border),
        ),
      ),
      child: Text(label,
          style: t.text.monoOf(highlighted ? t.colors.brand : t.colors.inkMuted,
              size: 11, weight: 600)),
    );
  }
}

class _GridCellView extends StatelessWidget {
  const _GridCellView({
    required this.width,
    required this.height,
    required this.cell,
    required this.selected,
    required this.showFormula,
    required this.onTap,
  });
  final double width, height;
  final GridCell? cell;
  final bool selected;
  final bool showFormula;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final c = cell;
    final text = c == null
        ? ''
        : showFormula && c.hasFormula
            ? '=${c.formula}'
            : (c.value ?? (c.hasFormula ? '=${c.formula}' : ''));
    final numeric = c?.value != null && double.tryParse(c!.value!) != null;
    return Semantics(
      label: c == null ? null : '${c.ref} $text',
      button: true,
      selected: selected,
      child: GestureDetector(
        onTap: onTap,
        behavior: HitTestBehavior.opaque,
        child: Container(
          width: width,
          height: height,
          padding: const EdgeInsets.symmetric(horizontal: HarborSpace.s2),
          alignment: numeric && !showFormula
              ? Alignment.centerRight
              : Alignment.centerLeft,
          decoration: BoxDecoration(
            color: selected
                ? t.colors.brandSoft
                : c?.hasFormula == true
                    ? t.colors.statusLocalFill.withValues(alpha: 0.35)
                    : Colors.transparent,
            border: Border(
              right: BorderSide(color: t.colors.borderSubtle),
              bottom: BorderSide(color: t.colors.borderSubtle),
            ),
          ),
          foregroundDecoration: selected
              ? BoxDecoration(
                  border: Border.all(
                      color: t.colors.brand, width: HarborStroke.focus))
              : null,
          child: Directionality(
            textDirection: TextDirection.ltr,
            child: Text(
              text,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: c?.hasFormula == true && showFormula
                  ? t.text.monoOf(t.colors.ink, size: 12)
                  : t.text.smallOf(t.colors.ink),
            ),
          ),
        ),
      ),
    );
  }
}

class _SheetTabs extends StatelessWidget {
  const _SheetTabs({
    required this.sheets,
    required this.active,
    required this.chartCount,
    required this.cellCount,
    required this.l10n,
  });
  final List<String> sheets;
  final String active;
  final int chartCount;
  final int cellCount;
  final AppLocalizations l10n;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final wc = HarborBreakpoints.of(context);
    return Container(
      decoration: BoxDecoration(
        color: t.colors.surfaceRaised,
        border: Border(top: BorderSide(color: t.colors.border)),
      ),
      padding: EdgeInsets.symmetric(
          horizontal: HarborBreakpoints.gutter(wc), vertical: HarborSpace.s2),
      child: Row(children: [
        Expanded(
          child: SingleChildScrollView(
            scrollDirection: Axis.horizontal,
            child: Row(children: [
              for (final s in sheets)
                Padding(
                  padding:
                      const EdgeInsetsDirectional.only(end: HarborSpace.s2),
                  child: Tooltip(
                    message: s == active ? '' : l10n.workSheetNotPreviewed,
                    child: ChoiceChip(
                      label: Text(s),
                      selected: s == active,
                      onSelected: s == active ? (_) {} : null,
                    ),
                  ),
                ),
            ]),
          ),
        ),
        const SizedBox(width: HarborSpace.s3),
        Wrap(spacing: HarborSpace.s2, children: [
          HarborPill(l10n.workCells(cellCount), icon: Icons.grid_on_outlined),
          if (chartCount > 0)
            HarborPill(l10n.workCharts(chartCount),
                icon: Icons.insert_chart_outlined),
        ]),
      ]),
    );
  }
}

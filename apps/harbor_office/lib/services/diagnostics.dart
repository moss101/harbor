import 'dart:async';

import 'package:flutter/foundation.dart';

import 'harbor_service.dart';

/// App-side half of diagnostics without telemetry (production plan C1).
///
/// Uncaught framework and zone errors are recorded into the core's
/// encrypted diagnostics log through `diag.record`. Nothing leaves the
/// device: the log is read back only by the user's own "Export
/// diagnostics" action. Errors raised before the core is open are held in
/// a bounded buffer and flushed once it is.
class DiagnosticsSink {
  DiagnosticsSink._();
  static final DiagnosticsSink instance = DiagnosticsSink._();

  static const _bufferLimit = 50;
  final List<Map<String, String>> _buffer = [];
  HarborService? _service;
  bool _installed = false;

  /// Hook `FlutterError.onError` and `PlatformDispatcher.onError`; keeps
  /// the previous handlers (console output in debug builds stays).
  void install() {
    if (_installed) return;
    _installed = true;
    final previousFlutter = FlutterError.onError;
    FlutterError.onError = (details) {
      record(
        level: 'error',
        message: details.exceptionAsString(),
        context: details.library ?? details.context?.toDescription(),
        stack: details.stack?.toString(),
      );
      previousFlutter?.call(details);
    };
    final previousPlatform = PlatformDispatcher.instance.onError;
    PlatformDispatcher.instance.onError = (error, stack) {
      record(
        level: 'error',
        message: error.toString(),
        context: 'PlatformDispatcher.onError',
        stack: stack.toString(),
      );
      return previousPlatform?.call(error, stack) ?? false;
    };
  }

  /// Bind the open core and flush anything recorded before it was ready.
  void attach(HarborService? service) {
    _service = service;
    if (service == null) return;
    final pending = List.of(_buffer);
    _buffer.clear();
    for (final r in pending) {
      unawaited(_send(service, r));
    }
  }

  /// Record one event. Never throws; never blocks the caller.
  void record({
    required String level,
    required String message,
    String? context,
    String? stack,
    String source = 'app',
  }) {
    final rec = <String, String>{
      'level': level,
      'source': source,
      'message': message,
      if (context != null) 'context': context,
      if (stack != null) 'stack': _trimStack(stack),
    };
    final service = _service;
    if (service == null) {
      if (_buffer.length >= _bufferLimit) _buffer.removeAt(0);
      _buffer.add(rec);
      return;
    }
    unawaited(_send(service, rec));
  }

  Future<void> _send(HarborService service, Map<String, String> rec) async {
    try {
      await service.recordDiagnostic(rec);
    } catch (_) {
      // Diagnostics must never turn one failure into two.
    }
  }

  static String _trimStack(String stack) {
    final lines = stack.split('\n');
    return lines.take(30).join('\n');
  }
}

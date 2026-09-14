/// Background-isolate worker for libharbor_ffi.
///
/// The native core handle lives on a dedicated isolate; every core call —
/// light or heavy — executes there, so the UI isolate never blocks on FFI
/// (no jank from model installs, ingestion, or generation). Requests are
/// the same JSON envelopes the synchronous binding uses; results cross
/// the isolate boundary as plain Dart values.
///
/// Protocol: the worker gets TWO main-isolate ports in its spawn config —
/// `ready` (replied to once its command port is listening, or with an
/// error envelope) and `responses` (carries every request reply). Main→
/// worker traffic flows over the command port the worker hands back.
///
/// Ownership: the worker closes the native handle when
/// [HarborCoreWorker.close] is called (or when the isolate is killed —
/// the OS reclaims the process memory either way).
library;

import 'dart:async';
import 'dart:convert';
import 'dart:ffi';
import 'dart:isolate';

import 'package:ffi/ffi.dart';

import 'harbor_ffi.dart' show HarborCoreException;

class _SpawnConfig {
  _SpawnConfig(this.ready, this.responses, this.libraryPath, this.dataRoot,
      this.workspaceId, this.privacyMode, this.deviceRootHex);
  final SendPort ready;
  final SendPort responses;
  final String libraryPath;
  final String dataRoot;
  final String workspaceId;
  final int privacyMode;
  final String? deviceRootHex;
}

/// A long-lived background isolate owning the native workspace handle.
class HarborCoreWorker {
  HarborCoreWorker._(this._commands, this.isolate, this._responses) {
    _responses.listen(_onMessage);
  }

  final SendPort _commands;
  final Isolate isolate;
  final ReceivePort _responses;
  final Map<int, Completer<Object?>> _pending = {};
  int _nextId = 0;
  bool _closed = false;

  /// Spawn the worker and open the native workspace inside it.
  static Future<HarborCoreWorker> spawn({
    required String libraryPath,
    required String dataRoot,
    required String workspaceId,
    required int privacyMode,
    String? deviceRootHex,
  }) async {
    final ready = ReceivePort();
    final responses = ReceivePort();
    final errors = ReceivePort();
    final isolate = await Isolate.spawn(
      _entry,
      _SpawnConfig(ready.sendPort, responses.sendPort, libraryPath, dataRoot,
          workspaceId, privacyMode, deviceRootHex),
      onError: errors.sendPort,
      errorsAreFatal: true,
    );
    final init = await ready.first;
    if (init is SendPort) {
      final worker = HarborCoreWorker._(init, isolate, responses);
      // Surface uncaught worker errors as failed pending requests.
      errors.listen((message) {
        final fail = _FailAll('worker crashed: $message');
        for (final completer in worker._pending.values) {
          if (!completer.isCompleted) completer.completeError(fail);
        }
        worker._pending.clear();
      });
      return worker;
    }
    // Anything else is an init error envelope.
    responses.close();
    final message =
        init is Map ? (init['error'] ?? 'open failed') : 'open failed';
    isolate.kill(priority: Isolate.beforeNextEvent);
    throw HarborCoreException(message.toString());
  }

  /// Invoke a core method on the worker isolate. Resolves with the raw
  /// result (map or list); throws [HarborCoreException] on error envelopes.
  Future<Object?> request(String method, [Map<String, dynamic>? args]) {
    if (_closed) {
      return Future.error(HarborCoreException('worker closed'));
    }
    final id = ++_nextId;
    final completer = Completer<Object?>();
    _pending[id] = completer;
    _commands.send({'id': id, 'method': method, 'args': args ?? const {}});
    return completer.future;
  }

  /// Close the native handle inside the worker and stop the isolate.
  Future<void> close() async {
    if (_closed) return;
    _closed = true;
    try {
      final id = ++_nextId;
      final completer = Completer<Object?>();
      _pending[id] = completer;
      _commands.send({'cmd': 'close', 'id': id});
      // Bounded wait: never let shutdown hang the app.
      await completer.future
          .timeout(const Duration(seconds: 5), onTimeout: () => null);
    } catch (_) {
      // The worker may already be gone; the OS reclaims process state.
    }
    isolate.kill(priority: Isolate.beforeNextEvent);
    _responses.close();
    for (final completer in _pending.values) {
      if (!completer.isCompleted) {
        completer.completeError(HarborCoreException('worker closed'));
      }
    }
    _pending.clear();
  }

  void _onMessage(dynamic message) {
    if (message is! Map) return;
    final id = message['id'] as int?;
    if (id == null) return;
    final completer = _pending.remove(id);
    if (completer == null) return;
    if (completer.isCompleted) return;
    if (message['ok'] == true) {
      completer.complete(message['result']);
    } else {
      completer.completeError(HarborCoreException(
          (message['error'] ?? 'unknown error').toString()));
    }
  }

  /// Worker-isolate entry: open the native library + handle, then serve
  /// requests until closed.
  static void _entry(_SpawnConfig config) {
    final commands = ReceivePort();
    DynamicLibrary? lib;
    Pointer<Void> handle = Pointer.fromAddress(0);
    _CallDart? call;
    _StringFreeDart? stringFree;
    _CloseDart? closeFn;
    try {
      try {
        lib = DynamicLibrary.open(config.libraryPath);
      } catch (_) {
        // iOS device builds force-load the core into the main executable.
        final process = DynamicLibrary.process();
        if (!process.providesSymbol('harbor_core_open')) {
          rethrow;
        }
        lib = process;
      }
      final openFn =
          lib.lookupFunction<_OpenNative, _OpenDart>('harbor_core_open_ex');
      final rootPtr = config.dataRoot.toNativeUtf8();
      final wsPtr = config.workspaceId.toNativeUtf8();
      final rootHexPtr = config.deviceRootHex == null
          ? nullptr
          : config.deviceRootHex!.toNativeUtf8();
      handle = openFn(rootPtr, wsPtr, config.privacyMode, rootHexPtr);
      malloc
        ..free(rootPtr)
        ..free(wsPtr);
      if (rootHexPtr != nullptr) malloc.free(rootHexPtr);
      if (handle == Pointer<Void>.fromAddress(0)) {
        throw HarborCoreException('harbor_core_open failed');
      }
      call = lib.lookupFunction<_CallNative, _CallDart>('harbor_core_call');
      stringFree = lib
          .lookupFunction<_StringFreeNative, _StringFreeDart>('harbor_core_string_free');
      closeFn = lib.lookupFunction<_CloseNative, _CloseDart>('harbor_core_close');
      commands.listen((message) {
        if (message is! Map) return;
        final id = message['id'] as int?;
        if (id == null) return;
        if (message['cmd'] == 'close') {
          if (handle != Pointer.fromAddress(0)) {
            closeFn!(handle);
            handle = Pointer.fromAddress(0);
          }
          commands.close();
          Isolate.exit();
        }
        final method = message['method'] as String?;
        if (method == null) return;
        final args =
            (message['args'] as Map?)?.cast<String, dynamic>() ?? const {};
        final request = jsonEncode({'method': method, 'args': args});
        final reqPtr = request.toNativeUtf8();
        final resp = call!(handle, reqPtr);
        malloc.free(reqPtr);
        if (resp == Pointer.fromAddress(0)) {
          config.responses
              .send({'id': id, 'ok': false, 'error': 'harbor_core_call returned null'});
          return;
        }
        final text = resp.toDartString();
        stringFree!(resp);
        try {
          final envelope = jsonDecode(text) as Map<dynamic, dynamic>;
          if (envelope['ok'] == true) {
            config.responses
                .send({'id': id, 'ok': true, 'result': envelope['result']});
          } else {
            config.responses.send({
              'id': id,
              'ok': false,
              'error': (envelope['error'] ?? 'unknown error').toString(),
            });
          }
        } catch (e) {
          config.responses.send({'id': id, 'ok': false, 'error': 'bad envelope: $e'});
        }
      });
      config.ready.send(commands.sendPort);
    } catch (e) {
      config.ready.send({'error': e.toString()});
    }
  }
}

/// All in-flight requests failed because the worker isolate died.
class _FailAll implements Exception {
  _FailAll(this.message);
  final String message;
  @override
  String toString() => message;
}

// FFI signature typedefs (shared shape with harbor_ffi.dart; duplicated
// here because the worker opens the library itself).
typedef _OpenNative = Pointer<Void> Function(
    Pointer<Utf8>, Pointer<Utf8>, Uint8, Pointer<Utf8>);
typedef _OpenDart = Pointer<Void> Function(
    Pointer<Utf8>, Pointer<Utf8>, int, Pointer<Utf8>);
typedef _CallNative = Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>);
typedef _CallDart = Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>);
typedef _StringFreeNative = Void Function(Pointer<Utf8>);
typedef _StringFreeDart = void Function(Pointer<Utf8>);
typedef _CloseNative = Void Function(Pointer<Void>);
typedef _CloseDart = void Function(Pointer<Void>);

/// Dart FFI bindings for Harbor Core (`libharbor_ffi`).
///
/// The boundary is a single JSON dispatcher (`harbor_core_call`) plus
/// lifecycle functions. All product policy (privacy, egress, approvals,
/// commit semantics) lives behind it in Rust; this file is transport only.
library;

import 'dart:convert';
import 'dart:ffi';
import 'dart:io';

import 'package:ffi/ffi.dart';

typedef _OpenNative = Pointer<Void> Function(Pointer<Utf8>, Pointer<Utf8>, Uint8);
typedef _OpenDart = Pointer<Void> Function(Pointer<Utf8>, Pointer<Utf8>, int);
typedef _CallNative = Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>);
typedef _CallDart = Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>);
typedef _StringFreeNative = Void Function(Pointer<Utf8>);
typedef _StringFreeDart = void Function(Pointer<Utf8>);
typedef _CloseNative = Void Function(Pointer<Void>);
typedef _CloseDart = void Function(Pointer<Void>);

/// Privacy modes mirror `harbor_security::policy::PrivacyMode`.
enum HarborPrivacyMode { localOnly, hybrid, remoteAllowed }

class HarborCoreException implements Exception {
  HarborCoreException(this.message);
  final String message;
  @override
  String toString() => 'HarborCoreException: $message';
}

/// Typed client over libharbor_ffi.
class HarborCoreClient {
  HarborCoreClient._(this._lib, this._handle);

  final DynamicLibrary _lib;
  final Pointer<Void> _handle;
  late final _CallDart _call =
      _lib.lookupFunction<_CallNative, _CallDart>('harbor_core_call');
  late final _StringFreeDart _stringFree =
      _lib.lookupFunction<_StringFreeNative, _StringFreeDart>('harbor_core_string_free');

  /// Open a library at [path] and a workspace handle.
  factory HarborCoreClient.open(String libraryPath, String dataRoot,
      String workspaceId, HarborPrivacyMode mode) {
    final lib = DynamicLibrary.open(libraryPath);
    final openFn = lib.lookupFunction<_OpenNative, _OpenDart>('harbor_core_open');
    final handle = openFn(
      dataRoot.toNativeUtf8(),
      workspaceId.toNativeUtf8(),
      mode.index,
    );
    if (handle == Pointer<Void>.fromAddress(0)) {
      throw HarborCoreException('harbor_core_open failed');
    }
    return HarborCoreClient._(lib, handle);
  }


  /// Invoke a core method; throws [HarborCoreException] on error envelope.
  Map<String, dynamic> call(String method, [Map<String, dynamic>? args]) {
    final request = jsonEncode({'method': method, 'args': args ?? {}});
    final reqPtr = request.toNativeUtf8();
    final resp = _call(_handle, reqPtr);
    malloc.free(reqPtr);
    if (resp == Pointer.fromAddress(0)) {
      throw HarborCoreException('harbor_core_call returned null');
    }
    final text = resp.toDartString();
    _stringFree(resp);
    final envelope = jsonDecode(text) as Map<String, dynamic>;
    if (envelope['ok'] != true) {
      throw HarborCoreException(envelope['error']?.toString() ?? 'unknown error');
    }
    return (envelope['result'] as Map).cast<String, dynamic>();
  }

  // Typed convenience methods -------------------------------------------

  Map<String, dynamic> createRun(String runId) =>
      call('run.create', {'run_id': runId});

  Map<String, dynamic> runState(String runId) => call('run.state', {'run_id': runId});

  Map<String, dynamic> pauseRun(String runId, {String reason = 'user'}) =>
      call('run.pause', {'run_id': runId, 'reason': reason});

  ({String blobId, int size}) putBlob(List<int> bytes) {
    final r = call('blob.put', {'data_b64': base64Encode(bytes)});
    return (blobId: r['blob_id'] as String, size: r['size'] as int);
  }

  List<int> getBlob(String blobId) {
    final r = call('blob.get', {'blob_id': blobId});
    return base64Decode(r['data_b64'] as String);
  }

  Map<String, dynamic> trustPulse() => call('trust.pulse');

  void close() {
    final closeFn =
        _lib.lookupFunction<_CloseNative, _CloseDart>('harbor_core_close');
    closeFn(_handle);
  }
}

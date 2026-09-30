// "Open in Harbor Office Suite": documents handed to the suite by the
// platform (Android VIEW intents, iOS "Open in…" / Files).
//
// The native embedding resolves the platform handle (content:// URI,
// security-scoped URL) to a readable COPY in a temp directory and tells
// Dart about it over `dev.harbor.office/open`:
//
//   Dart → native  getInitialPath   the document the app was LAUNCHED with
//                                   (consumed once; null otherwise)
//   native → Dart  openPath         a document delivered to the RUNNING app
//
// Both carry `{path, name}` or `{error}`. Dart then validates the type,
// imports the copy into the suite's own Documents/Opened store (so the
// Work recents entry stays reopenable after the temp copy is cleaned),
// and emits an [OpenRequest] the Work surface consumes.
//
// Nothing here trusts the sender: the name is re-sanitized, only the four
// types the core can preview are accepted, and a file that is not a real
// document is refused by the core's own preview, not by an extension.
import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';

/// Extensions the core can preview (mirrors `artifact.preview`'s PDF +
/// OOXML dispatch). Markdown/plain text are NOT here on purpose: they go
/// through Convert, not through "open".
const supportedOpenExtensions = {'docx', 'xlsx', 'pptx', 'pdf'};

/// Why an incoming document could not be opened.
enum OpenFailure {
  /// Extension outside [supportedOpenExtensions].
  unsupportedType,

  /// The platform could not hand over a readable copy (revoked grant,
  /// provider error, file too large).
  unreadable,
}

/// A document to open, or the reason it cannot be.
@immutable
class OpenRequest {
  const OpenRequest.file({required this.path, required this.name})
      : failure = null;
  const OpenRequest.failed(this.failure, {this.name = ''}) : path = null;

  final String? path;
  final String name;
  final OpenFailure? failure;

  bool get ok => failure == null;

  @override
  String toString() => ok ? 'OpenRequest($name @ $path)' : 'OpenRequest($failure)';
}

/// The lowercase extension of [name] without the dot ('' when none).
String fileExtension(String name) {
  final dot = name.lastIndexOf('.');
  return dot < 0 ? '' : name.substring(dot + 1).toLowerCase();
}

/// A display name safe to use as a file name: basename only, no control
/// characters or separators, no leading dots, bounded length.
String sanitizeIncomingName(String raw) {
  var name = raw.split(RegExp(r'[\\/]')).last;
  name = name.replaceAll(RegExp(r'[\x00-\x1f<>:"|?*]'), '_').trim();
  name = name.replaceFirst(RegExp(r'^\.+'), '');
  if (name.length > 120) {
    final ext = fileExtension(name);
    final keep = ext.isEmpty ? 120 : 120 - ext.length - 1;
    final stem = name.substring(0, name.length - (ext.isEmpty ? 0 : ext.length + 1));
    name = ext.isEmpty
        ? stem.substring(0, keep)
        : '${stem.substring(0, keep)}.$ext';
  }
  return name;
}

class OpenIntake {
  OpenIntake({MethodChannel? channel, Future<Directory> Function()? openedDir})
      : _channel = channel ?? const MethodChannel(channelName),
        _openedDir = openedDir ?? _defaultOpenedDir;

  static const channelName = 'dev.harbor.office/open';

  final MethodChannel _channel;
  final Future<Directory> Function() _openedDir;
  final _controller = StreamController<OpenRequest>.broadcast();
  bool _started = false;

  static Future<Directory> _defaultOpenedDir() async {
    final docs = await getApplicationDocumentsDirectory();
    return Directory('${docs.path}${Platform.pathSeparator}Opened')
        .create(recursive: true);
  }

  /// Documents delivered while the app runs, and (once) the launch
  /// document.
  Stream<OpenRequest> get requests => _controller.stream;

  /// Begin listening for warm-start deliveries and fetch the launch
  /// document. Safe to call once; platforms without the channel (desktop,
  /// tests) simply never emit.
  Future<void> start() async {
    if (_started) return;
    _started = true;
    _channel.setMethodCallHandler((call) async {
      if (call.method == 'openPath') {
        _emit(await _import(call.arguments));
      }
      return null;
    });
    try {
      final initial = await _channel.invokeMethod<Object?>('getInitialPath');
      if (initial != null) _emit(await _import(initial));
    } on MissingPluginException {
      // No native open-with on this platform.
    } on PlatformException {
      // A failing native side is "nothing to open", not a crash.
    }
  }

  void _emit(OpenRequest r) {
    if (!_controller.isClosed) _controller.add(r);
  }

  Future<void> dispose() async {
    _channel.setMethodCallHandler(null);
    await _controller.close();
  }

  /// Validate the native payload and move the copy into the suite's own
  /// store. Never throws: every failure becomes an [OpenRequest.failed].
  Future<OpenRequest> _import(Object? payload) async {
    if (payload is! Map) return const OpenRequest.failed(OpenFailure.unreadable);
    if (payload['error'] != null) {
      return OpenRequest.failed(OpenFailure.unreadable,
          name: sanitizeIncomingName('${payload['name'] ?? ''}'));
    }
    final srcPath = payload['path'];
    final rawName = payload['name'];
    if (srcPath is! String || srcPath.isEmpty) {
      return const OpenRequest.failed(OpenFailure.unreadable);
    }
    final name = sanitizeIncomingName(
        rawName is String && rawName.isNotEmpty
            ? rawName
            : srcPath.split(Platform.pathSeparator).last);
    final src = File(srcPath);
    if (!supportedOpenExtensions.contains(fileExtension(name))) {
      await _discard(src);
      return OpenRequest.failed(OpenFailure.unsupportedType, name: name);
    }
    try {
      final bytes = await src.readAsBytes();
      final dir = await _openedDir();
      final dest = await _destinationFor(dir, name, bytes);
      if (dest.existsSync() == false) {
        await dest.writeAsBytes(bytes, flush: true);
      }
      await _discard(src);
      return OpenRequest.file(path: dest.path, name: name);
    } catch (_) {
      await _discard(src);
      return OpenRequest.failed(OpenFailure.unreadable, name: name);
    }
  }

  /// Re-opening the same document must not litter `Opened/` with "(2)"
  /// copies: an existing file with identical bytes is reused; a different
  /// document with the same name gets the next free name.
  Future<File> _destinationFor(
      Directory dir, String name, Uint8List bytes) async {
    final first = File('${dir.path}${Platform.pathSeparator}$name');
    var candidate = first;
    var i = 2;
    while (candidate.existsSync()) {
      if (await _sameBytes(candidate, bytes)) return candidate;
      final dot = name.lastIndexOf('.');
      final stem = dot <= 0 ? name : name.substring(0, dot);
      final ext = dot <= 0 ? '' : name.substring(dot);
      candidate = File('${dir.path}${Platform.pathSeparator}$stem ($i)$ext');
      i++;
    }
    return candidate;
  }

  Future<bool> _sameBytes(File f, Uint8List bytes) async {
    if (await f.length() != bytes.length) return false;
    return listEquals(await f.readAsBytes(), bytes);
  }

  /// Remove the native temp copy and its per-delivery folder.
  Future<void> _discard(File f) async {
    try {
      if (f.existsSync()) await f.delete();
      final parent = f.parent;
      if (parent.path.contains('open-intake') && parent.listSync().isEmpty) {
        await parent.delete();
      }
    } catch (_) {}
  }
}

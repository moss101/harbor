import 'dart:io';

/// Where a saved/converted copy lands when the platform has no save
/// picker (mobile): the first free `name`, then `name (2)`, `name (3)`…
/// A silent overwrite is exactly how a user loses an approved file, and
/// a name collision the UI did not surface loses the approved output
/// (found on the iOS simulator).
String firstFreePath(
    String dir, String name, bool Function(String path) exists) {
  final sep = Platform.pathSeparator;
  String at(String n) => '$dir$sep$n';
  if (!exists(at(name))) return at(name);
  final dot = name.lastIndexOf('.');
  final stem = dot <= 0 ? name : name.substring(0, dot);
  final ext = dot <= 0 ? '' : name.substring(dot);
  for (var i = 2;; i++) {
    final candidate = at('$stem ($i)$ext');
    if (!exists(candidate)) return candidate;
  }
}

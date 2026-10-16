import 'package:flutter/foundation.dart';

import '../../services/file_passages.dart';

/// A place in the open file the canvas should bring into view and mark —
/// set when a citation of an answer about the file is tapped. [serial]
/// makes a second tap on the same citation jump again.
@immutable
final class WorkFocus {
  const WorkFocus(this.target, this.serial);
  final PassageTarget target;
  final int serial;
}

/// What the Work views listen to (null: nothing to show).
typedef WorkFocusListenable = ValueListenable<WorkFocus?>;

import 'dart:io';
import 'package:harbor_native/harbor_worker.dart';

Future<void> main() async {
  final support = Platform.environment['HOME']!;
  final dataRoot = '$support/Library/Application Support/dev.harbor.harbor_app/harbor-data';
  await Directory(dataRoot).create(recursive: true);
  // Session 1: create a durable run.
  final w1 = await HarborCoreWorker.spawn(
      libraryPath: '/Users/mohsin/projects/harbor/core/target/debug/libharbor_ffi.dylib',
      dataRoot: dataRoot, workspaceId: 'ws-persist', privacyMode: 0);
  final id1 = (await w1.request('identity.get')) as Map;
  await w1.request('run.create', {'run_id': 'run-persist-check'});
  await w1.request('run.log_request', {'run_id': 'run-persist-check', 'text': 'persistence probe'});
  await w1.close();
  // Session 2: reopen — identity and run must survive.
  final w2 = await HarborCoreWorker.spawn(
      libraryPath: '/Users/mohsin/projects/harbor/core/target/debug/libharbor_ffi.dylib',
      dataRoot: dataRoot, workspaceId: 'ws-persist', privacyMode: 0);
  final id2 = (await w2.request('identity.get')) as Map;
  final runs = (await w2.request('runs.list')) as Map;
  print('identity same: ${id1['device_id'] == id2['device_id']} (${id2['device_id']})');
  print('runs survive: ${runs['runs'].any((r) => r['run_id'] == 'run-persist-check')}');
  await w2.close();
}
